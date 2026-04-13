use crate::bin_core::constants::{ LEAD, MAX_TOKENS, NAG_THRESHOLD, TOKEN_THRESHOLD};
use crate::bin_core::dispatch::dispatch_tool;
use crate::bin_core::state::State;
use serde_json::Value as Json;

/// Main agent loop that orchestrates LLM calls and tool execution.
pub async fn agent_loop(state: &State, messages: &mut Vec<Json>, system: &str) {
    let tools = state.tools();
    let mut rounds_since_todo = 0usize;

    loop {
        // Context compression
        state.compactor.micro_compact(messages);
        if ContextCompactor::estimate_tokens(messages) > TOKEN_THRESHOLD {
            eprintln!("[auto-compact triggered]");
            *messages = state.compactor.auto_compact(messages).await;
        }

        // Drain background notifications
        let notifs = state.bg.drain_notifications();
        if !notifs.is_empty() {
            let txt: Vec<String> = notifs
                .iter()
                .map(|n| {
                    let preview = if n.result.chars().count() > 500 {
                        format!("{}...", n.result.chars().take(500).collect::<String>())
                    } else {
                        n.result.clone()
                    };
                    format!("[bg:{}] {}: {}", n.task_id, n.status, preview)
                })
                .collect();
            messages.push(serde_json::json!({
                "role": "user",
                "content": format!("<background-results>\n{}\n</background-results>", txt.join("\n"))
            }));
        }

        // Check lead inbox
        let inbox = state.bus.read_inbox(LEAD);
        if !inbox.is_empty() {
            messages.push(serde_json::json!({
                "role": "user",
                "content": format!("<inbox>{}</inbox>", serde_json::to_string_pretty(&inbox).unwrap_or_default())
            }));
        }

        // LLM call
        let response = match state
            .client
            .create_message(
                &state.model,
                Some(system),
                messages,
                Some(&tools),
                MAX_TOKENS,
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error: {e}");
                return;
            }
        };

        messages.push(serde_json::json!({
            "role": "assistant",
            "content": response["content"]
        }));

        if response["stop_reason"] != "tool_use" {
            return;
        }

        // Tool execution
        let mut results: Vec<Json> = Vec::new();
        let mut used_todo = false;
        let mut manual_compact = false;

        if let Some(content) = response["content"].as_array() {
            for block in content {
                if block["type"] == "tool_use" {
                    let tool_name = block["name"].as_str().unwrap_or("");
                    let input = &block["input"];

                    if tool_name == "compact" {
                        manual_compact = true;
                    }

                    let output = dispatch_tool(state, tool_name, input);

                    eprintln!("> {tool_name}:");
                    let preview = if output.chars().count() > 200 {
                        format!("{}...", output.chars().take(200).collect::<String>())
                    } else {
                        output.clone()
                    };
                    eprintln!("{preview}");

                    results.push(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": block["id"],
                        "content": output
                    }));

                    if tool_name == "TodoWrite" {
                        used_todo = true;
                    }
                }
            }
        }

        // Todo nag reminder
        if used_todo {
            rounds_since_todo = 0;
        } else {
            rounds_since_todo += 1;
        }
        let has_open = {
            let todo = state.todo.lock().unwrap();
            todo.items().iter().any(|t| t.status != "completed")
        };
        if has_open && rounds_since_todo >= NAG_THRESHOLD {
            results.push(serde_json::json!({
                "type": "text",
                "text": "<reminder>Update your todos.</reminder>"
            }));
        }

        messages.push(serde_json::json!({
            "role": "user",
            "content": results
        }));

        // Manual compact
        if manual_compact {
            eprintln!("[manual compact]");
            *messages = state.compactor.auto_compact(messages).await;
            return;
        }
    }
}

// Re-export for use in this module
use crate::context_compact::ContextCompactor;
