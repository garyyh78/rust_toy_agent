//! s01_agent_loop.rs - The Agent Loop
//!
//! The entire secret of an AI coding agent in one pattern:
//!
//!     while stop_reason == "tool_use":
//!         response = LLM(messages, tools)
//!         execute tools
//!         append results
//!
//!     +----------+      +-------+      +---------+
//!     |   User   | ---> |  LLM  | ---> |  Tool   |
//!     |  prompt  |      |       |      | execute |
//!     +----------+      +---+---+      +----+----+
//!                           ^               |
//!                           |   tool_result |
//!                           +---------------+
//!                           (loop continues)
//!
//! Key insight: feed tool results back to the model until it stops.

use serde_json::json;
use serde_json::Value as Json;
use std::env;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command as Proc;

type Messages = Vec<Json>;

const TOOLS: &str = r#"[{
    "name": "bash",
    "description": "Run a shell command.",
    "input_schema": {
        "type": "object",
        "properties": {"command": {"type": "string"}},
        "required": ["command"]
    }
}]"#;

// ── Anthropic API client ─────────────────────────────────────────────────────

struct AnthropicClient {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl AnthropicClient {
    fn from_env() -> Self {
        let base_url = env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
        let api_key = env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        Self {
            api_key,
            base_url,
            client: reqwest::Client::new(),
        }
    }

    async fn create_message(
        &self,
        model: &str,
        system: Option<&str>,
        messages: &[Json],
        tools: Option<&Json>,
        max_tokens: u32,
    ) -> Json {
        let url = format!("{}/v1/messages", self.base_url);
        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens
        });
        if let Some(sys) = system {
            if !sys.is_empty() {
                body["system"] = Json::String(sys.to_string());
            }
        }
        if let Some(t) = tools {
            if !t.as_array().map_or(true, |a| a.is_empty()) {
                body["tools"] = t.clone();
            }
        }

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .expect("HTTP request failed");

        let status = resp.status();
        let text = resp.text().await.expect("Failed to read response body");
        if !status.is_success() {
            eprintln!("\x1b[31m[api] error {}: {}\x1b[0m", status, text);
            panic!("Anthropic API error {}: {}", status, text);
        }
        serde_json::from_str(&text).expect("Failed to parse API response")
    }
}

// ── Shell / file tools ────────────────────────────────────────────────────────

fn run_bash(command: &str, workdir: &Path) -> String {
    let blocked = ["rm -rf /", "sudo", "shutdown", "reboot", "> /dev/"];
    if blocked.iter().any(|b| command.contains(b)) {
        eprintln!("\x1b[31m[tool:bash] BLOCKED: {}\x1b[0m", command);
        return "Error: Dangerous command blocked".to_string();
    }
    match Proc::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(workdir)
        .output()
    {
        Err(e) => format!("Error: {}", e),
        Ok(out) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let text = text.trim().to_string();
            if text.is_empty() {
                "(no output)".to_string()
            } else if text.len() > 50000 {
                text[..50000].to_string()
            } else {
                text
            }
        }
    }
}

// ── REPL helper ───────────────────────────────────────────────────────────────

fn read_prompt(prompt: &str) -> Option<String> {
    print!("{}", prompt);
    std::io::stdout().flush().ok();
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim().to_string()),
    }
}

fn print_final_response(messages: &[Json]) {
    if let Some(last) = messages.last() {
        if let Some(blocks) = last["content"].as_array() {
            for block in blocks {
                if block["type"] == "text" {
                    if let Some(text) = block["text"].as_str() {
                        println!("{}", text);
                    }
                }
            }
        }
    }
}

// ── Logging helpers ────────────────────────────────────────────────────────────

fn log_section(title: &str) {
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    eprintln!("\x1b[34m {}\x1b[0m", title);
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
}

fn log_info(label: &str, value: &str) {
    eprintln!("\x1b[36m  {:<12}\x1b[0m {}", label, value);
}

fn log_step(step: &str, detail: &str) {
    eprintln!("\x1b[33m  {}\x1b[0m {}", step, detail);
}

fn log_output_preview(output: &str) {
    let lines: Vec<&str> = output.lines().take(5).collect();
    let truncated = output.lines().count() > 5;
    for line in &lines {
        eprintln!("\x1b[90m    {}\x1b[0m", line);
    }
    if truncated {
        eprintln!("\x1b[90m    ... ({} more lines)\x1b[0m", output.lines().count() - 5);
    }
}

// ── The agent loop ─────────────────────────────────────────────────────────────

async fn agent_loop(
    client: &AnthropicClient,
    model: &str,
    system: &str,
    tools: &Json,
    messages: &mut Messages,
    workdir: &PathBuf,
) {
    let mut round = 0usize;
    loop {
        round += 1;
        log_section(&format!("Agent Loop Round {}", round));
        log_info("history", &format!("{} messages", messages.len()));
        log_info("model", model);
        eprintln!();

        log_step("→", "Calling Anthropic API...");
        let response = client
            .create_message(model, Some(system), messages, Some(tools), 8000)
            .await;

        let stop_reason = response["stop_reason"].as_str().unwrap_or("").to_string();
        let content = response["content"].clone();

        let usage = &response["usage"];
        let input_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
        let output_tokens = usage["output_tokens"].as_u64().unwrap_or(0);
        log_info("tokens", &format!("{} in / {} out", input_tokens, output_tokens));
        log_info("stop", &stop_reason);
        eprintln!();

        messages.push(json!({"role": "assistant", "content": content}));

        if stop_reason != "tool_use" {
            log_section("Agent Response");
            log_info("status", "Complete - no tool use");
            return;
        }

        let tool_count = content.as_array()
            .map(|blocks| blocks.iter()
                .filter(|b| b["type"] == "tool_use")
                .count())
            .unwrap_or(0);

        log_info("tools", &format!("{} tool call(s) requested", tool_count));
        eprintln!();

        let mut results = Vec::new();
        if let Some(blocks) = content.as_array() {
            for (i, block) in blocks.iter().enumerate() {
                if block["type"] == "tool_use" {
                    let cmd = block["input"]["command"].as_str().unwrap_or("");
                    let tool_id = block["id"].as_str().unwrap_or("unknown");

                    log_step(&format!("[{}]", i + 1), &format!("bash: \x1b[1m{}\x1b[0m", cmd));
                    log_info("id", &tool_id[..std::cmp::min(8, tool_id.len())]);

                    let output = run_bash(cmd, workdir);
                    let output_len = output.len();

                    log_info("output", &format!("{} bytes", output_len));
                    log_output_preview(&output);
                    eprintln!();

                    results.push(json!({
                        "type": "tool_result",
                        "tool_use_id": block["id"],
                        "content": output
                    }));
                }
            }
        }

        log_info("results", &format!("{} tool result(s) ready", results.len()));
        messages.push(json!({"role": "user", "content": results}));
    }
}

// ── Main ───────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let workdir = env::current_dir().unwrap();
    let client = AnthropicClient::from_env();
    let model = env::var("MODEL_ID").expect("MODEL_ID not set");
    let system = format!(
        "You are a coding agent at {}. Use bash to solve tasks. Act, don't explain.",
        workdir.display()
    );
    let tools: Json = serde_json::from_str(TOOLS).unwrap();

    eprintln!();
    eprintln!("\x1b[35m╔══════════════════════════════════════════════════════════════╗\x1b[0m");
    eprintln!("\x1b[35m║          S01 Agent Loop - Interactive Session                ║\x1b[0m");
    eprintln!("\x1b[35m╚══════════════════════════════════════════════════════════════╝\x1b[0m");
    eprintln!();
    log_info("model", &model);
    log_info("workdir", &workdir.display().to_string());
    log_info("tools", "bash");
    log_info("max_tokens", "8000");
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    eprintln!();

    let mut history: Messages = Vec::new();
    let mut turn = 0usize;

    loop {
        let query = match read_prompt("\x1b[36ms01 >> \x1b[0m") {
            None => break,
            Some(q) => q,
        };
        if matches!(query.as_str(), "q" | "exit" | "") {
            eprintln!("\x1b[35m  Session ended.\x1b[0m");
            break;
        }

        turn += 1;
        eprintln!();
        eprintln!("\x1b[35m  Turn {}: {}\x1b[0m", turn, &query[..std::cmp::min(50, query.len())]);
        eprintln!();

        history.push(json!({"role": "user", "content": query}));
        agent_loop(&client, &model, &system, &tools, &mut history, &workdir).await;
        print_final_response(&history);
        println!();
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tools_json_parsing() {
        let tools: Json = serde_json::from_str(TOOLS).unwrap();
        assert!(tools.is_array());
        let arr = tools.as_array().unwrap();
        assert_eq!(arr.len(), 1);

        let bash_tool = &arr[0];
        assert_eq!(bash_tool["name"], "bash");
        assert_eq!(bash_tool["description"], "Run a shell command.");

        let schema = &bash_tool["input_schema"];
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["command"].is_object());

        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "command");
    }

    #[test]
    fn test_run_bash_simple_echo() {
        let workdir = env::current_dir().unwrap();
        let output = run_bash("echo hello", &workdir);
        assert!(output.contains("hello"));
    }

    #[test]
    fn test_run_bash_dangerous_blocked() {
        let workdir = env::current_dir().unwrap();
        let dangerous = [
            "rm -rf /",
            "sudo ls",
            "shutdown -h now",
            "reboot",
            "cat /dev/null > /dev/sda",
        ];

        for cmd in dangerous {
            let output = run_bash(cmd, &workdir);
            assert!(output.contains("Dangerous command blocked"), "Failed to block: {}", cmd);
        }
    }

    #[test]
    fn test_run_bash_no_output() {
        let workdir = env::current_dir().unwrap();
        let output = run_bash("true", &workdir);
        assert_eq!(output, "(no output)");
    }

    #[test]
    fn test_run_bash_captures_stderr() {
        let workdir = env::current_dir().unwrap();
        let output = run_bash("ls /nonexistent 2>&1", &workdir);
        assert!(output.contains("No such file") || output.contains("cannot access"));
    }

    #[test]
    fn test_tool_result_structure() {
        let tool_use_id = "test-123";
        let output = "test output";

        let result = json!({
            "type": "tool_result",
            "tool_use_id": tool_use_id,
            "content": output
        });

        assert_eq!(result["type"], "tool_result");
        assert_eq!(result["tool_use_id"], tool_use_id);
        assert_eq!(result["content"], output);
    }

    #[test]
    fn test_messages_append_structure() {
        let mut messages: Messages = Vec::new();

        messages.push(json!({"role": "user", "content": "test query"}));
        assert_eq!(messages.len(), 1);

        let tool_response = json!({
            "role": "assistant",
            "content": [
                {
                    "type": "tool_use",
                    "id": "tool-1",
                    "name": "bash",
                    "input": {"command": "echo test"}
                }
            ]
        });
        messages.push(tool_response);
        assert_eq!(messages.len(), 2);

        let results = vec![json!({
            "type": "tool_result",
            "tool_use_id": "tool-1",
            "content": "test output"
        })];
        messages.push(json!({"role": "user", "content": results}));
        assert_eq!(messages.len(), 3);
    }

    #[test]
    fn test_stop_reason_check() {
        let test_cases = vec![
            ("tool_use", true),
            ("end_turn", false),
            ("max_tokens", false),
            ("", false),
        ];

        for (reason, should_continue) in test_cases {
            let should_stop = reason != "tool_use";
            assert_eq!(should_stop, !should_continue, "Failed for reason: {}", reason);
        }
    }

    #[test]
    fn test_system_prompt_format() {
        let workdir = PathBuf::from("/test/path");
        let system = format!(
            "You are a coding agent at {}. Use bash to solve tasks. Act, don't explain.",
            workdir.display()
        );
        assert!(system.contains("/test/path"));
        assert!(system.contains("Use bash to solve tasks"));
    }

    #[test]
    fn test_exit_commands() {
        let exit_commands = ["q", "exit", ""];
        for cmd in exit_commands {
            assert!(matches!(cmd, "q" | "exit" | ""), "Failed for command: {}", cmd);
        }
    }

    #[test]
    fn test_tool_use_block_parsing() {
        let response = json!({
            "stop_reason": "tool_use",
            "content": [
                {
                    "type": "text",
                    "text": "Let me run a command."
                },
                {
                    "type": "tool_use",
                    "id": "tool-456",
                    "name": "bash",
                    "input": {"command": "ls -la"}
                }
            ]
        });

        let mut results = Vec::new();
        if let Some(blocks) = response["content"].as_array() {
            for block in blocks {
                if block["type"] == "tool_use" {
                    let cmd = block["input"]["command"].as_str().unwrap_or("");
                    results.push(json!({
                        "type": "tool_result",
                        "tool_use_id": block["id"],
                        "content": format!("output for: {}", cmd)
                    }));
                }
            }
        }

        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["tool_use_id"], "tool-456");
        assert!(results[0]["content"].as_str().unwrap().contains("ls -la"));
    }
}
