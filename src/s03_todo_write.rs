//! s03_todo_write.rs - TodoWrite
//!
//! The model tracks its own progress via a TodoManager. A nag reminder
//! forces it to keep updating when it forgets.
//!
//!     +----------+      +-------+      +---------+
//!     |   User   | ---> |  LLM  | ---> | Tools   |
//!     |  prompt  |      |       |      | + todo  |
//!     +----------+      +---+---+      +----+----+
//!                           ^               |
//!                           |   tool_result |
//!                           +---------------+
//!                                 |
//!                     +-----------+-----------+
//!                     | TodoManager state     |
//!                     | [ ] task A            |
//!                     | [>] task B <- doing   |
//!                     | [x] task C            |
//!                     +-----------------------+
//!                                 |
//!                     if rounds_since_todo >= 3:
//!                       inject <reminder>
//!
//! Key insight: "The agent can track its own progress -- and I can see it."

use rust_toy_agent::client::AnthropicClient;
use rust_toy_agent::tools::{dispatch_tools, TodoManager, TOOLS};
use serde_json::json;
use serde_json::Value as Json;
use std::env;
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

type Messages = Vec<Json>;

// -- Logging helpers --

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
        eprintln!(
            "\x1b[90m    ... ({} more lines)\x1b[0m",
            output.lines().count() - 5
        );
    }
}

// -- Agent loop with nag reminder --

async fn agent_loop(
    client: &AnthropicClient,
    model: &str,
    system: &str,
    tools: &Json,
    messages: &mut Messages,
    workdir: &Path,
    todo: &Arc<Mutex<TodoManager>>,
) {
    let mut round = 0usize;
    let mut rounds_since_todo = 0usize;
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
        log_info(
            "tokens",
            &format!("{} in / {} out", input_tokens, output_tokens),
        );
        log_info("stop", &stop_reason);
        eprintln!();

        messages.push(json!({"role": "assistant", "content": content}));

        if stop_reason != "tool_use" {
            log_section("Agent Response");
            log_info("status", "Complete - no tool use");
            return;
        }

        let tool_count = content
            .as_array()
            .map(|blocks| blocks.iter().filter(|b| b["type"] == "tool_use").count())
            .unwrap_or(0);

        log_info("tools", &format!("{} tool call(s) requested", tool_count));
        eprintln!();

        let mut results = Vec::new();
        let mut used_todo = false;

        if let Some(blocks) = content.as_array() {
            for (i, block) in blocks.iter().enumerate() {
                if block["type"] == "tool_use" {
                    let tool_name = block["name"].as_str().unwrap_or("unknown");
                    let tool_id = block["id"].as_str().unwrap_or("unknown");

                    log_step(
                        &format!("[{}]", i + 1),
                        &format!("{}: \x1b[1m{:?}\x1b[0m", tool_name, block["input"]),
                    );
                    log_info("id", &tool_id[..std::cmp::min(8, tool_id.len())]);

                    let (output, did_todo) =
                        dispatch_tools(tool_name, &block["input"], workdir, todo);
                    let output = output.unwrap_or_else(|| format!("Unknown tool: {}", tool_name));
                    if did_todo {
                        used_todo = true;
                    }

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

        rounds_since_todo = if used_todo {
            0
        } else {
            rounds_since_todo + 1
        };

        if rounds_since_todo >= 3 {
            log_step("⚠", "Injecting nag reminder");
            results.insert(
                0,
                json!({"type": "text", "text": "<reminder>Update your todos.</reminder>"}),
            );
        }

        log_info("results", &format!("{} tool result(s) ready", results.len()));
        messages.push(json!({"role": "user", "content": results}));
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

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let workdir = env::current_dir().unwrap();
    let client = AnthropicClient::from_env();
    let model = env::var("MODEL_ID").expect("MODEL_ID not set");
    let system = format!(
        "You are a coding agent at {}. \
Use the todo tool to plan multi-step tasks. Mark in_progress before starting, completed when done. \
Prefer tools over prose.",
        workdir.display()
    );
    let tools: Json = serde_json::from_str(TOOLS).unwrap();
    let todo = Arc::new(Mutex::new(TodoManager::new()));

    eprintln!();
    eprintln!("\x1b[35m╔══════════════════════════════════════════════════════════════╗\x1b[0m");
    eprintln!("\x1b[35m║          S03 Agent Loop - TodoWrite Edition                 ║\x1b[0m");
    eprintln!("\x1b[35m╚══════════════════════════════════════════════════════════════╝\x1b[0m");
    eprintln!();
    log_info("model", &model);
    log_info("workdir", &workdir.display().to_string());
    log_info("tools", "bash, read_file, write_file, edit_file, todo");
    log_info("max_tokens", "8000");
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    eprintln!();

    let mut history: Messages = Vec::new();
    let mut turn = 0usize;

    loop {
        let query = match read_prompt("\x1b[36ms03 >> \x1b[0m") {
            None => break,
            Some(q) => q,
        };
        if matches!(query.as_str(), "q" | "exit" | "") {
            eprintln!("\x1b[35m  Session ended.\x1b[0m");
            break;
        }

        turn += 1;
        eprintln!();
        eprintln!(
            "\x1b[35m  Turn {}: {}\x1b[0m",
            turn,
            &query[..std::cmp::min(50, query.len())]
        );
        eprintln!();

        history.push(json!({"role": "user", "content": query}));
        agent_loop(
            &client,
            &model,
            &system,
            &tools,
            &mut history,
            &workdir,
            &todo,
        )
        .await;
        print_final_response(&history);
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nag_reminder_threshold() {
        let mut rounds_since_todo = 0usize;
        rounds_since_todo += 1;
        assert!(rounds_since_todo < 3);
        rounds_since_todo += 1;
        assert!(rounds_since_todo < 3);
        rounds_since_todo += 1;
        assert!(rounds_since_todo >= 3);
        rounds_since_todo = 0;
        assert_eq!(rounds_since_todo, 0);
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
    fn test_messages_flow() {
        let mut messages: Messages = Vec::new();
        messages.push(json!({"role": "user", "content": "Hello"}));
        assert_eq!(messages.len(), 1);
        messages.push(json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "Hi"}]
        }));
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_system_prompt_format() {
        let workdir = std::path::PathBuf::from("/test/path");
        let system = format!(
            "You are a coding agent at {}. \
Use the todo tool to plan multi-step tasks. Mark in_progress before starting, completed when done. \
Prefer tools over prose.",
            workdir.display()
        );
        assert!(system.contains("/test/path"));
        assert!(system.contains("todo tool"));
    }
}
