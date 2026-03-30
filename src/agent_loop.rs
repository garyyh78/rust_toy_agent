//! agent_loop.rs - Core agent loop with nag reminder
//!
//! The loop calls the LLM, dispatches tool calls, and tracks todo usage.
//! If the LLM skips todo updates for 3+ rounds, a nag reminder is injected.
//!
//! Module relationship:
//!
//!   ┌─────────────┐        ┌──────────────┐
//!   │   client.rs │◄───────│ agent_loop   │
//!   └─────────────┘        └──┬───┬───┬───┘
//!        create_message()     │   │   │
//!                             │   │   │
//!   ┌─────────────┐           │   │   │
//!   │   tools.rs  │◄──────────┘   │   │
//!   └─────────────┘  dispatch      │   │
//!       dispatch_tools()           │   │
//!                                  │   │
//!   ┌─────────────┐               │   │
//!   │  logger.rs  │◄──────────────┘   │
//!   └─────────────┘  log_*()          │
//!                                     │
//!   ┌─────────────┐                   │
//!   │help_utils.rs│◄──────────────────┘
//!   └─────────────┘  (called by tools)
//!
//! agent_loop() flow:
//!
//!   ┌──────────────────────────────────────────────┐
//!   │  loop {                                      │
//!   │    call LLM  ──→  check stop_reason          │
//!   │       │                                       │
//!   │       ├─ not "tool_use"  ──→  return          │
//!   │       │                                       │
//!   │       └─ "tool_use"     ──→  dispatch_tools() │
//!   │              │                                │
//!   │              ├─ track rounds_since_todo       │
//!   │              ├─ if >= 3: inject <reminder>    │
//!   │              └─ append tool_result to msgs    │
//!   │  }                                            │
//!   └──────────────────────────────────────────────┘

use crate::client::AnthropicClient;
use crate::logger::SessionLogger;
use crate::todo_manager::TodoManager;
use crate::tools::dispatch_tools;
use serde_json::json;
use serde_json::Value as Json;
use std::path::Path;
use std::sync::{Arc, Mutex};

const MAX_TOKENS: u32 = 16 * 1024;

/// The conversation history is a vector of JSON message objects.
pub type Messages = Vec<Json>;

// -- Agent loop with nag reminder --
// Each iteration: call LLM, collect tool_use blocks, dispatch them,
// and append tool_result blocks back into the conversation.
// The "nag" counter injects a reminder if the LLM forgets its todos.

#[allow(clippy::too_many_arguments)]
/// Validate that every tool_use block has a matching tool_result immediately after.
/// Returns an error description if pairing is broken.
fn validate_tool_pairing(messages: &[Json]) -> Option<String> {
    for i in 0..messages.len() - 1 {
        if messages[i]["role"] == "assistant" {
            if let Some(blocks) = messages[i]["content"].as_array() {
                for block in blocks {
                    if block["type"] == "tool_use" {
                        let tool_id = block["id"].as_str().unwrap_or("unknown");
                        let next = &messages[i + 1];
                        let has_result = next["content"]
                            .as_array()
                            .map(|arr| {
                                arr.iter().any(|b| {
                                    b["type"] == "tool_result"
                                        && b["tool_use_id"].as_str() == Some(tool_id)
                                })
                            })
                            .unwrap_or(false);
                        if !has_result {
                            return Some(format!(
                                "tool_use {tool_id} at index {i} has no matching tool_result at index {}",
                                i + 1
                            ));
                        }
                    }
                }
            }
        }
    }
    None
}

/// Keep only the last N conversation rounds to prevent history bloat.
/// Always preserves the first user message (index 0).
fn truncate_messages(messages: &mut Messages, max_rounds: usize) {
    // Each round = 2 messages (assistant + user tool_result)
    let max_messages = 1 + max_rounds * 2; // first user + N rounds
    if messages.len() > max_messages {
        let remove_count = messages.len() - max_messages;
        messages.drain(1..1 + remove_count);
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn agent_loop(
    client: &AnthropicClient,
    model: &str,
    system: &str,
    tools: &Json,
    messages: &mut Messages,
    workdir: &Path,
    todo: &Arc<Mutex<TodoManager>>,
    logger: &mut SessionLogger,
) -> (u64, u64, u32) {
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut round = 0usize;
    let mut rounds_since_todo = 0usize;
    loop {
        round += 1;
        logger.log_section(&format!("Agent Loop Round {round}"));
        logger.log_info("history", &format!("{} messages", messages.len()));
        logger.log_info("model", model);
        eprintln!();

        // Build the request body, log it, then send
        logger.log_step("→", &format!("Calling Agent Model ({model})..."));

        // Validate tool_use/tool_result pairing before sending
        if let Some(err) = validate_tool_pairing(messages) {
            logger.log_section("History Validation Error");
            eprintln!("\x1b[31m  {err}\x1b[0m");
            logger.log_info("status", "Corrupted history, stopping loop");
            return (0, 0, 0);
        }

        // Truncate old messages to keep conversation size manageable
        truncate_messages(messages, 8);

        let body = AnthropicClient::build_request_body(
            model,
            Some(system),
            messages,
            Some(tools),
            MAX_TOKENS,
        );
        let body_json = serde_json::to_string(&body).unwrap_or_default();
        let body_len = body_json.len();
        logger.log_info("request_size", &format!("{} bytes", body_len));
        logger.log_info("max_tokens", &MAX_TOKENS.to_string());
        logger.log_api_request(&body);

        let response = match client.send_body(&body).await {
            Ok(r) => {
                logger.log_api_response(&r);
                r
            }
            Err(e) => {
                logger.log_api_error(&e);
                logger.log_section("Agent Error");
                eprintln!("\x1b[31m  {e}\x1b[0m");
                // Try to extract structured error from API JSON body
                if let Some(pos) = e.find('{') {
                    if let Ok(parsed) = serde_json::from_str::<Json>(&e[pos..]) {
                        let err = &parsed["error"];
                        if let Some(msg) = err["message"].as_str() {
                            logger.log_info("message", msg);
                            eprintln!("\x1b[31m  message: {msg}\x1b[0m");
                        }
                        if let Some(err_type) = err["type"].as_str() {
                            logger.log_info("type", err_type);
                        }
                        if let Some(code) = err["code"].as_str() {
                            logger.log_info("code", code);
                        }
                        if let Some(param) = err["param"].as_str() {
                            logger.log_info("param", param);
                        }
                    }
                }
                logger.log_info("status", "API call failed, stopping loop");
                return (total_input_tokens, total_output_tokens, round as u32);
            }
        };

        // Extract stop_reason to decide whether to continue the loop
        let stop_reason = response["stop_reason"].as_str().unwrap_or("").to_string();
        let content = response["content"].clone();

        // Log token usage for observability
        let usage = &response["usage"];
        let input_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
        let output_tokens = usage["output_tokens"].as_u64().unwrap_or(0);
        total_input_tokens += input_tokens;
        total_output_tokens += output_tokens;
        logger.log_info(
            "tokens",
            &format!("{input_tokens} in / {output_tokens} out"),
        );
        logger.log_info("stop", &stop_reason);
        eprintln!();

        // Append the assistant's response to conversation history
        messages.push(json!({"role": "assistant", "content": content}));

        // If the model didn't request tool use, we're done
        if stop_reason != "tool_use" {
            logger.log_section("Agent Response");
            logger.log_info("status", "Complete - no tool use");
            return (total_input_tokens, total_output_tokens, round as u32);
        }

        let tool_count = content
            .as_array()
            .map(|blocks| blocks.iter().filter(|b| b["type"] == "tool_use").count())
            .unwrap_or(0);

        logger.log_info("tools", &format!("{tool_count} tool call(s) requested"));
        eprintln!();

        // Iterate over content blocks and dispatch each tool_use
        let mut results = Vec::new();
        let mut used_todo = false;

        if let Some(blocks) = content.as_array() {
            for (i, block) in blocks.iter().enumerate() {
                if block["type"] == "tool_use" {
                    let tool_name = block["name"].as_str().unwrap_or("unknown");
                    let tool_id = block["id"].as_str().unwrap_or("unknown");

                    logger.log_step(
                        &format!("[{}]", i + 1),
                        &format!("{tool_name}: \x1b[1m{:?}\x1b[0m", block["input"]),
                    );
                    logger.log_info("id", &tool_id[..std::cmp::min(8, tool_id.len())]);

                    let (output, did_todo) =
                        dispatch_tools(tool_name, &block["input"], workdir, todo);
                    let output = output.unwrap_or_else(|| format!("Unknown tool: {tool_name}"));
                    if did_todo {
                        used_todo = true;
                    }

                    logger.log_info("output", &format!("{} bytes", output.len()));
                    logger.log_output_preview(&output);
                    eprintln!();

                    // Each result becomes a tool_result block in the next message
                    results.push(json!({
                        "type": "tool_result",
                        "tool_use_id": block["id"],
                        "content": output
                    }));
                }
            }
        }

        // Track how many rounds since the last todo update
        rounds_since_todo = if used_todo { 0 } else { rounds_since_todo + 1 };

        // Nag reminder: append to the last tool_result's content
        // (not as a separate text block, which breaks tool_use/tool_result pairing)
        if rounds_since_todo >= 3 && !results.is_empty() {
            logger.log_step("⚠", "Injecting nag reminder into tool_result");
            if let Some(last) = results.last_mut() {
                if let Some(content) = last["content"].as_str() {
                    let updated =
                        format!("{content}\n\n<reminder>Update your todos.</reminder>");
                    last["content"] = json!(updated);
                }
            }
        }

        logger.log_info(
            "results",
            &format!("{} tool result(s) ready", results.len()),
        );
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
            assert_ne!(reason, "tool_use", "should stop for: {reason}");
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

    // -- Tests for the 400 error fix: tool_use without tool_result --

    #[test]
    fn test_corrupted_history_detection() {
        // Simulate the exact scenario that caused the 400 error:
        // assistant message with tool_use blocks, but no tool_result follows
        let messages: Vec<Json> = vec![
            json!({"role": "user", "content": "list files"}),
            json!({
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "call_00_abc123", "name": "bash", "input": {"command": "ls"}}
                ]
            }),
            // Missing tool_result here! This is the corruption.
            json!({"role": "user", "content": "next question"}),
        ];

        // Detect: find assistant messages with tool_use not followed by tool_result
        for i in 0..messages.len() - 1 {
            if messages[i]["role"] == "assistant" {
                if let Some(blocks) = messages[i]["content"].as_array() {
                    let has_tool_use = blocks.iter().any(|b| b["type"] == "tool_use");
                    if has_tool_use {
                        let next = &messages[i + 1];
                        let has_tool_result = next["content"]
                            .as_array()
                            .map(|arr| arr.iter().any(|b| b["type"] == "tool_result"))
                            .unwrap_or(false);
                        assert!(
                            !has_tool_result,
                            "This history is corrupted: tool_use at index {i} has no tool_result at index {}",
                            i + 1
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_valid_tool_use_followed_by_tool_result() {
        // Correct pairing: tool_use followed by tool_result
        let messages: Vec<Json> = vec![
            json!({"role": "user", "content": "list files"}),
            json!({
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "call_00_abc", "name": "bash", "input": {"command": "ls"}}
                ]
            }),
            json!({
                "role": "user",
                "content": [
                    {"type": "tool_result", "tool_use_id": "call_00_abc", "content": "file1.txt"}
                ]
            }),
        ];

        // Verify pairing: every tool_use has a matching tool_result next
        for i in 0..messages.len() - 1 {
            if messages[i]["role"] == "assistant" {
                if let Some(blocks) = messages[i]["content"].as_array() {
                    for block in blocks {
                        if block["type"] == "tool_use" {
                            let tool_id = block["id"].as_str().unwrap();
                            let next = &messages[i + 1];
                            let has_match = next["content"]
                                .as_array()
                                .map(|arr| {
                                    arr.iter().any(|b| {
                                        b["type"] == "tool_result"
                                            && b["tool_use_id"].as_str() == Some(tool_id)
                                    })
                                })
                                .unwrap_or(false);
                            assert!(
                                has_match,
                                "tool_use {tool_id} at index {i} has no matching tool_result"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_multiple_tool_use_all_need_results() {
        // Multiple tool_use blocks in one assistant message
        let tool_id_1 = "call_00_first";
        let tool_id_2 = "call_01_second";
        let messages: Vec<Json> = vec![
            json!({"role": "user", "content": "do two things"}),
            json!({
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": tool_id_1, "name": "bash", "input": {"command": "ls"}},
                    {"type": "tool_use", "id": tool_id_2, "name": "bash", "input": {"command": "pwd"}}
                ]
            }),
            json!({
                "role": "user",
                "content": [
                    {"type": "tool_result", "tool_use_id": tool_id_1, "content": "files"},
                    {"type": "tool_result", "tool_use_id": tool_id_2, "content": "/home"}
                ]
            }),
        ];

        // Check that ALL tool_use ids have matching tool_result
        let assistant = &messages[1];
        let user_result = &messages[2];
        if let Some(blocks) = assistant["content"].as_array() {
            for block in blocks {
                if block["type"] == "tool_use" {
                    let id = block["id"].as_str().unwrap();
                    let found = user_result["content"]
                        .as_array()
                        .map(|arr| arr.iter().any(|b| b["tool_use_id"].as_str() == Some(id)))
                        .unwrap_or(false);
                    assert!(found, "Missing tool_result for {id}");
                }
            }
        }
    }

    #[test]
    fn test_nag_reminder_resets_after_todo() {
        let mut rounds_since_todo = 0usize;
        // 3 rounds without todo
        rounds_since_todo += 1;
        rounds_since_todo += 1;
        rounds_since_todo += 1;
        assert!(rounds_since_todo >= 3, "Should trigger nag");
        // After todo is used, counter resets
        rounds_since_todo = 0;
        assert_eq!(rounds_since_todo, 0, "Should reset after todo use");
        // 2 more rounds still under threshold
        rounds_since_todo += 1;
        rounds_since_todo += 1;
        assert!(rounds_since_todo < 3, "Should not trigger nag yet");
    }

    #[test]
    fn test_nag_reminder_appended_to_tool_result() {
        // Simulate what agent_loop does when rounds_since_todo >= 3
        let mut results = vec![json!({
            "type": "tool_result",
            "tool_use_id": "call_123",
            "content": "[ ] #1: Write tests\n(0/1 completed)"
        })];

        // Inject reminder into last tool_result (not as a separate block)
        if let Some(last) = results.last_mut() {
            if let Some(content) = last["content"].as_str() {
                let updated = format!("{content}\n\n<reminder>Update your todos.</reminder>");
                last["content"] = json!(updated);
            }
        }

        // Must still be exactly 1 result (no extra text blocks)
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["type"], "tool_result");
        assert_eq!(results[0]["tool_use_id"], "call_123");
        let content = results[0]["content"].as_str().unwrap();
        assert!(content.contains("[ ] #1: Write tests"));
        assert!(content.contains("<reminder>Update your todos.</reminder>"));
    }

    #[test]
    fn test_nag_reminder_skipped_when_no_results() {
        // If there are no tool results, don't inject anything
        let mut results: Vec<Json> = vec![];
        let rounds_since_todo = 3usize;

        if rounds_since_todo >= 3 && !results.is_empty() {
            // This block should NOT execute
            panic!("Should not inject reminder when results is empty");
        }

        assert!(results.is_empty());
    }

    #[test]
    fn test_nag_reminder_all_results_are_tool_result_type() {
        // After reminder injection, every block must still be tool_result
        let mut results = vec![
            json!({
                "type": "tool_result",
                "tool_use_id": "call_a",
                "content": "output a"
            }),
            json!({
                "type": "tool_result",
                "tool_use_id": "call_b",
                "content": "output b"
            }),
        ];

        // Reminder goes into the LAST result only
        if let Some(last) = results.last_mut() {
            if let Some(content) = last["content"].as_str() {
                let updated = format!("{content}\n\n<reminder>Update your todos.</reminder>");
                last["content"] = json!(updated);
            }
        }

        // All blocks are still tool_result
        for r in &results {
            assert_eq!(r["type"], "tool_result", "All blocks must be tool_result");
            assert!(r["tool_use_id"].is_string(), "Must have tool_use_id");
        }
        // First result is unchanged
        assert_eq!(results[0]["content"], "output a");
        // Last result has the reminder
        assert!(results[1]["content"].as_str().unwrap().contains("<reminder>"));
    }

    // -- API error message extraction tests --

    fn extract_api_error(error_str: &str) -> Option<Json> {
        // Find the first '{' to extract the JSON body from the error string
        if let Some(pos) = error_str.find('{') {
            serde_json::from_str::<Json>(&error_str[pos..]).ok()
        } else {
            None
        }
    }

    #[test]
    fn test_extract_400_tool_use_error() {
        let error_str = r#"Anthropic API error 400 Bad Request: {"error":{"message":"messages.18: `tool_use` ids were found without `tool_result` blocks immediately after: call_00_BcFuzGiYOXapzyCLqM0pSMoh. Each `tool_use` block must have a corresponding `tool_result` block in the next message.","type":"invalid_request_error","param":null,"code":"invalid_request_error"}}"#;

        let parsed = extract_api_error(error_str).expect("Should parse JSON");
        let err = &parsed["error"];
        assert!(err["message"]
            .as_str()
            .unwrap()
            .contains("tool_use` ids were found without `tool_result`"));
        assert_eq!(err["type"], "invalid_request_error");
        assert_eq!(err["code"], "invalid_request_error");
        assert!(err["param"].is_null());
    }

    #[test]
    fn test_extract_401_auth_error() {
        let error_str = r#"Anthropic API error 401 Unauthorized: {"error":{"message":"Invalid API key","type":"authentication_error","code":"invalid_api_key"}}"#;

        let parsed = extract_api_error(error_str).expect("Should parse JSON");
        let err = &parsed["error"];
        assert_eq!(err["message"], "Invalid API key");
        assert_eq!(err["type"], "authentication_error");
        assert_eq!(err["code"], "invalid_api_key");
    }

    #[test]
    fn test_extract_429_rate_limit_error() {
        let error_str = r#"Anthropic API error 429 Too Many Requests: {"error":{"message":"Rate limit exceeded","type":"rate_limit_error","code":"rate_limit_exceeded"}}"#;

        let parsed = extract_api_error(error_str).expect("Should parse JSON");
        let err = &parsed["error"];
        assert_eq!(err["message"], "Rate limit exceeded");
        assert_eq!(err["type"], "rate_limit_error");
    }

    #[test]
    fn test_extract_non_json_error_returns_none() {
        let error_str = "HTTP request failed: connection refused";
        let parsed = extract_api_error(error_str);
        assert!(parsed.is_none());
    }

    #[test]
    fn test_extract_error_message_fields() {
        // Verify we can extract each field independently
        let error_str = r#"Anthropic API error 400: {"error":{"message":"bad request","type":"invalid_request_error","param":"messages[0].content","code":"invalid_value"}}"#;

        let parsed = extract_api_error(error_str).expect("Should parse JSON");
        let err = &parsed["error"];
        assert_eq!(err["message"].as_str().unwrap(), "bad request");
        assert_eq!(err["type"].as_str().unwrap(), "invalid_request_error");
        assert_eq!(err["param"].as_str().unwrap(), "messages[0].content");
        assert_eq!(err["code"].as_str().unwrap(), "invalid_value");
    }
}
