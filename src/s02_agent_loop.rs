//! s02_agent_loop.rs - Tools
//!
//! The agent loop from s01 didn't change. We just added tools to the array
//! and a dispatch map to route calls.
//!
//!     +----------+      +-------+      +------------------+
//!     |   User   | ---> |  LLM  | ---> | Tool Dispatch    |
//!     |  prompt  |      |       |      | {                |
//!     +----------+      +---+---+      |   bash: run_bash |
//!                           ^          |   read: run_read |
//!                           |          |   write: run_wr  |
//!                           +----------+   edit: run_edit |
//!                           tool_result| }                |
//!                                      +------------------+
//!
//! Key insight: "The loop didn't change at all. I just added tools."

use serde_json::json;
use serde_json::Value as Json;
use std::env;
use std::io::{BufRead, Write};
use std::path::{Component, Path, PathBuf};
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
}, {
    "name": "read_file",
    "description": "Read file contents.",
    "input_schema": {
        "type": "object",
        "properties": {
            "path": {"type": "string"},
            "limit": {"type": "integer"}
        },
        "required": ["path"]
    }
}, {
    "name": "write_file",
    "description": "Write content to file.",
    "input_schema": {
        "type": "object",
        "properties": {
            "path": {"type": "string"},
            "content": {"type": "string"}
        },
        "required": ["path", "content"]
    }
}, {
    "name": "edit_file",
    "description": "Replace exact text in file.",
    "input_schema": {
        "type": "object",
        "properties": {
            "path": {"type": "string"},
            "old_text": {"type": "string"},
            "new_text": {"type": "string"}
        },
        "required": ["path", "old_text", "new_text"]
    }
}]"#;

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
            if !t.as_array().is_none_or(|a| a.is_empty()) {
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

fn normalize_path(path: &Path) -> PathBuf {
    let mut components: Vec<Component> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                components.pop();
            }
            c => components.push(c),
        }
    }
    components.iter().collect()
}

fn safe_path(p: &str, workdir: &Path) -> Result<PathBuf, String> {
    let workdir_abs = workdir.canonicalize().unwrap_or_else(|_| workdir.to_path_buf());
    let raw = workdir_abs.join(p);
    let normalized = normalize_path(&raw);
    if normalized.starts_with(&workdir_abs) {
        Ok(normalized)
    } else {
        Err(format!("Path escapes workspace: {}", p))
    }
}

fn run_bash(command: &str, workdir: &Path) -> String {
    let blocked = ["rm -rf /", "sudo", "shutdown", "reboot", "> /dev/"];
    if blocked.iter().any(|b| command.contains(b)) {
        return "Error: Dangerous command blocked".to_string();
    }
    match Proc::new("sh").arg("-c").arg(command).current_dir(workdir).output() {
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

fn run_read(path: &str, limit: Option<usize>, workdir: &Path) -> String {
    match safe_path(path, workdir) {
        Err(e) => format!("Error: {}", e),
        Ok(fp) => match std::fs::read_to_string(&fp) {
            Err(e) => format!("Error: {}", e),
            Ok(text) => {
                let lines: Vec<&str> = text.lines().collect();
                let result: String = match limit {
                    Some(n) if n < lines.len() => {
                        let mut v: Vec<String> =
                            lines[..n].iter().map(|s| s.to_string()).collect();
                        v.push(format!("... ({} more lines)", lines.len() - n));
                        v.join("\n")
                    }
                    _ => lines.join("\n"),
                };
                if result.len() > 50000 {
                    result[..50000].to_string()
                } else {
                    result
                }
            }
        },
    }
}

fn run_write(path: &str, content: &str, workdir: &Path) -> String {
    match safe_path(path, workdir) {
        Err(e) => format!("Error: {}", e),
        Ok(fp) => {
            if let Some(parent) = fp.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::write(&fp, content) {
                Ok(_) => format!("Wrote {} bytes to {}", content.len(), path),
                Err(e) => format!("Error: {}", e),
            }
        }
    }
}

fn run_edit(path: &str, old_text: &str, new_text: &str, workdir: &Path) -> String {
    match safe_path(path, workdir) {
        Err(e) => format!("Error: {}", e),
        Ok(fp) => match std::fs::read_to_string(&fp) {
            Err(e) => format!("Error: {}", e),
            Ok(content) => {
                if !content.contains(old_text) {
                    return format!("Error: Text not found in {}", path);
                }
                let new_content = content.replacen(old_text, new_text, 1);
                match std::fs::write(&fp, new_content) {
                    Ok(_) => format!("Edited {}", path),
                    Err(e) => format!("Error: {}", e),
                }
            }
        },
    }
}

fn dispatch_tools(tool_name: &str, input: &Json, workdir: &Path) -> Option<String> {
    match tool_name {
        "bash" => Some(run_bash(input["command"].as_str().unwrap_or(""), workdir)),
        "read_file" => Some(run_read(
            input["path"].as_str().unwrap_or(""),
            input["limit"].as_u64().map(|n| n as usize),
            workdir,
        )),
        "write_file" => Some(run_write(
            input["path"].as_str().unwrap_or(""),
            input["content"].as_str().unwrap_or(""),
            workdir,
        )),
        "edit_file" => Some(run_edit(
            input["path"].as_str().unwrap_or(""),
            input["old_text"].as_str().unwrap_or(""),
            input["new_text"].as_str().unwrap_or(""),
            workdir,
        )),
        _ => None,
    }
}

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

async fn agent_loop(
    client: &AnthropicClient,
    model: &str,
    system: &str,
    tools: &Json,
    messages: &mut Messages,
    workdir: &Path,
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
            .map(|blocks| blocks.iter().filter(|b| b["type"] == "tool_use").count())
            .unwrap_or(0);

        log_info("tools", &format!("{} tool call(s) requested", tool_count));
        eprintln!();

        let mut results = Vec::new();
        if let Some(blocks) = content.as_array() {
            for (i, block) in blocks.iter().enumerate() {
                if block["type"] == "tool_use" {
                    let tool_name = block["name"].as_str().unwrap_or("unknown");
                    let tool_id = block["id"].as_str().unwrap_or("unknown");

                    log_step(&format!("[{}]", i + 1), &format!("{}: \x1b[1m{:?}\x1b[0m", tool_name, block["input"]));
                    log_info("id", &tool_id[..std::cmp::min(8, tool_id.len())]);

                    let output = dispatch_tools(tool_name, &block["input"], workdir)
                        .unwrap_or_else(|| format!("Unknown tool: {}", tool_name));

                    log_info("output", &format!("{} bytes", output.len()));
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

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let workdir = env::current_dir().unwrap();
    let client = AnthropicClient::from_env();
    let model = env::var("MODEL_ID").expect("MODEL_ID not set");
    let system = format!(
        "You are a coding agent at {}. Use tools to solve tasks. Act, don't explain.",
        workdir.display()
    );
    let tools: Json = serde_json::from_str(TOOLS).unwrap();

    eprintln!();
    eprintln!("\x1b[35m╔══════════════════════════════════════════════════════════════╗\x1b[0m");
    eprintln!("\x1b[35m║          S02 Agent Loop - Tools Edition                     ║\x1b[0m");
    eprintln!("\x1b[35m╚══════════════════════════════════════════════════════════════╝\x1b[0m");
    eprintln!();
    log_info("model", &model);
    log_info("workdir", &workdir.display().to_string());
    log_info("tools", "bash, read_file, write_file, edit_file");
    log_info("max_tokens", "8000");
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    eprintln!();

    let mut history: Messages = Vec::new();
    let mut turn = 0usize;

    loop {
        let query = match read_prompt("\x1b[36ms02 >> \x1b[0m") {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tools_json_parsing() {
        let tools: Json = serde_json::from_str(TOOLS).unwrap();
        assert!(tools.is_array());
        let arr = tools.as_array().unwrap();
        assert_eq!(arr.len(), 4);
        
        let tool_names: Vec<&str> = arr.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(tool_names, vec!["bash", "read_file", "write_file", "edit_file"]);
    }

    #[test]
    fn test_bash_tool_schema() {
        let tools: Json = serde_json::from_str(TOOLS).unwrap();
        let bash = &tools.as_array().unwrap()[0];
        assert_eq!(bash["name"], "bash");
        let schema = &bash["input_schema"];
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["command"].is_object());
    }

    #[test]
    fn test_read_file_tool_schema() {
        let tools: Json = serde_json::from_str(TOOLS).unwrap();
        let read = &tools.as_array().unwrap()[1];
        assert_eq!(read["name"], "read_file");
        let schema = &read["input_schema"];
        let props = &schema["properties"];
        assert!(props["path"].is_object());
        assert!(props["limit"].is_object());
    }

    #[test]
    fn test_write_file_tool_schema() {
        let tools: Json = serde_json::from_str(TOOLS).unwrap();
        let write = &tools.as_array().unwrap()[2];
        assert_eq!(write["name"], "write_file");
    }

    #[test]
    fn test_edit_file_tool_schema() {
        let tools: Json = serde_json::from_str(TOOLS).unwrap();
        let edit = &tools.as_array().unwrap()[3];
        assert_eq!(edit["name"], "edit_file");
    }

    #[test]
    fn test_system_prompt_format() {
        let workdir = PathBuf::from("/test/path");
        let system = format!(
            "You are a coding agent at {}. Use tools to solve tasks. Act, don't explain.",
            workdir.display()
        );
        assert!(system.contains("/test/path"));
    }

    #[test]
    fn test_tool_result_structure() {
        let result = json!({
            "type": "tool_result",
            "tool_use_id": "test-id-123",
            "content": "tool output"
        });
        assert_eq!(result["type"], "tool_result");
    }

    #[test]
    fn test_tool_dispatch_unknown() {
        let input = json!({"foo": "bar"});
        let output = dispatch_tools("unknown_tool", &input, &PathBuf::from("."));
        assert!(output.is_none());
    }

    #[test]
    fn test_messages_flow() {
        let mut messages: Messages = Vec::new();
        messages.push(json!({"role": "user", "content": "Hello"}));
        assert_eq!(messages.len(), 1);
        messages.push(json!({"role": "assistant", "content": [{"type": "text", "text": "Hi"}]}));
        assert_eq!(messages.len(), 2);
    }
}