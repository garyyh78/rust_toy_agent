//! agent_loop.rs - Core agent loop with nag reminder
//!
//! The loop calls the LLM, dispatches tool calls, and tracks todo usage.
//! If the LLM skips todo updates for 3+ rounds, a nag reminder is injected.

use crate::client::AnthropicClient;
use crate::tools::{dispatch_tools, TodoManager};
use serde_json::json;
use serde_json::Value as Json;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub type Messages = Vec<Json>;

// -- Logging helpers --

pub fn log_section(title: &str) {
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    eprintln!("\x1b[34m {}\x1b[0m", title);
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
}

pub fn log_info(label: &str, value: &str) {
    eprintln!("\x1b[36m  {:<12}\x1b[0m {}", label, value);
}

pub fn log_step(step: &str, detail: &str) {
    eprintln!("\x1b[33m  {}\x1b[0m {}", step, detail);
}

pub fn log_output_preview(output: &str) {
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

pub async fn agent_loop(
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
    fn test_messages_append_flow() {
        let mut messages: Messages = Vec::new();
        messages.push(json!({"role": "user", "content": "Hello"}));
        assert_eq!(messages.len(), 1);
        messages.push(json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "Hi"}]
        }));
        assert_eq!(messages.len(), 2);
        let last = messages.last().unwrap();
        assert_eq!(last["role"], "assistant");
    }

    #[test]
    fn test_stop_reason_handling() {
        let reasons = ["end_turn", "max_tokens", "stop_sequence", ""];
        for reason in reasons {
            assert_ne!(reason, "tool_use", "should stop for: {}", reason);
        }
        assert_eq!("tool_use", "tool_use");
    }

    #[test]
    fn test_system_prompt_format() {
        let workdir = std::path::PathBuf::from("/test/project");
        let system = format!(
            "You are a coding agent at {}. \
Use the todo tool to plan multi-step tasks. Mark in_progress before starting, completed when done. \
Prefer tools over prose.",
            workdir.display()
        );
        assert!(system.contains("/test/project"));
        assert!(system.contains("todo tool"));
        assert!(system.contains("in_progress"));
        assert!(system.contains("completed"));
    }

    #[test]
    fn test_tool_result_json_structure() {
        let result = json!({
            "type": "tool_result",
            "tool_use_id": "test-id-123",
            "content": "tool output"
        });
        assert_eq!(result["type"], "tool_result");
        assert_eq!(result["tool_use_id"], "test-id-123");
        assert_eq!(result["content"], "tool output");
    }
}
