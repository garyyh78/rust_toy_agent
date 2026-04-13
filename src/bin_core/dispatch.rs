use crate::background_tasks::BackgroundManager;
use crate::bin_core::state::State;
use crate::llm_client::AnthropicClient;
use crate::tool_runners::{run_bash, run_edit, run_read, run_write};
use serde_json::Value as Json;

/// Dispatch a tool call by name. Returns the output string.
pub fn dispatch_tool(state: &State, name: &str, input: &Json) -> String {
    let wd = &state.workdir;
    match name {
        "bash" => run_bash(input["command"].as_str().unwrap_or(""), wd),
        "read_file" => run_read(
            input["path"].as_str().unwrap_or(""),
            input["limit"].as_u64().map(|n| n as usize),
            wd,
        ),
        "write_file" => run_write(
            input["path"].as_str().unwrap_or(""),
            input["content"].as_str().unwrap_or(""),
            wd,
        ),
        "edit_file" => run_edit(
            input["path"].as_str().unwrap_or(""),
            input["old_text"].as_str().unwrap_or(""),
            input["new_text"].as_str().unwrap_or(""),
            wd,
        ),
        "TodoWrite" => {
            let items = input["items"]
                .as_array()
                .map(|a| a.as_slice())
                .unwrap_or(&[]);
            let mut mgr = state.todo.lock().unwrap();
            match mgr.update(items) {
                Ok(r) => r,
                Err(e) => format!("Error: {e}"),
            }
        }
        "task" => {
            let prompt = input["prompt"].as_str().unwrap_or("");
            let desc = input["description"].as_str().unwrap_or("subtask");
            let preview = if prompt.chars().count() > 80 {
                format!("{}...", prompt.chars().take(80).collect::<String>())
            } else {
                prompt.to_string()
            };
            eprintln!("  > task ({desc}): {preview}");

            // Note: This runs the subagent synchronously since dispatch_tool is sync.
            // The nested runtime issue from [1] is avoided by blocking on the runtime
            // that spawned this task, which works because we're not inside an async
            // context that already holds a runtime reference.
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(state.subagent.run_subagent(prompt))
        }
        "load_skill" => state
            .skills
            .get_content(input["name"].as_str().unwrap_or("")),
        "compact" => "Compacting...".to_string(),
        "background_run" => state.bg.run(input["command"].as_str().unwrap_or(""), wd),
        "check_background" => state.bg.check(input["task_id"].as_str()),
        "task_create" => {
            let mut mgr = state.task_mgr.lock().unwrap();
            match mgr.create(
                input["subject"].as_str().unwrap_or(""),
                input["description"].as_str().unwrap_or(""),
            ) {
                Ok(r) => r,
                Err(e) => format!("Error: {e}"),
            }
        }
        "task_get" => {
            let mgr = state.task_mgr.lock().unwrap();
            match mgr.get(input["task_id"].as_u64().unwrap_or(0) as u32) {
                Ok(r) => r,
                Err(e) => format!("Error: {e}"),
            }
        }
        "task_update" => {
            let mut mgr = state.task_mgr.lock().unwrap();
            match mgr.update(
                input["task_id"].as_u64().unwrap_or(0) as u32,
                input["status"].as_str(),
                input["add_blocked_by"].as_array().map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_u64().map(|n| n as u32))
                        .collect()
                }),
                input["add_blocks"].as_array().map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_u64().map(|n| n as u32))
                        .collect()
                }),
            ) {
                Ok(r) => r,
                Err(e) => format!("Error: {e}"),
            }
        }
        "task_list" => {
            let mgr = state.task_mgr.lock().unwrap();
            mgr.list_all()
        }
        "spawn_teammate" => {
            let name = input["name"].as_str().unwrap_or("");
            let role = input["role"].as_str().unwrap_or("");
            let prompt = input["prompt"].as_str().unwrap_or("");
            let mut team = state.team.lock().unwrap();
            match team.spawn(name, role) {
                Ok(msg) => {
                    // Clone data for the spawned thread
                    let client =
                        AnthropicClient::new(&state.client.api_key, &state.client.base_url);
                    let model = state.model.clone();
                    let workdir = state.workdir.clone();
                    let bus = Arc::clone(&state.bus);
                    let protocols = state.protocols.clone();
                    let _bg = BackgroundManager::new();

                    let name_owned = name.to_string();
                    let role_owned = role.to_string();
                    let prompt_owned = prompt.to_string();
                    let team_name = team.team_name().to_string();

                    std::thread::spawn(move || {
                        crate::bin_core::teammate::teammate_loop(
                            client,
                            model,
                            workdir,
                            bus,
                            protocols,
                            &name_owned,
                            &role_owned,
                            &prompt_owned,
                            &team_name,
                        );
                    });
                    msg
                }
                Err(e) => format!("Error: {e}"),
            }
        }
        "list_teammates" => {
            let team = state.team.lock().unwrap();
            team.list_all()
        }
        "send_message" => {
            let to = input["to"].as_str().unwrap_or("");
            let content = input["content"].as_str().unwrap_or("");
            let msg_type = input["msg_type"].as_str().unwrap_or("message");
            match state.bus.send("lead", to, content, msg_type) {
                Ok(r) => r,
                Err(e) => format!("Error: {e}"),
            }
        }
        "read_inbox" => {
            let msgs = state.bus.read_inbox("lead");
            serde_json::to_string_pretty(&msgs).unwrap_or_default()
        }
        "broadcast" => {
            let team = state.team.lock().unwrap();
            let names = team.member_names();
            match state
                .bus
                .broadcast("lead", input["content"].as_str().unwrap_or(""), &names)
            {
                Ok(r) => r,
                Err(e) => format!("Error: {e}"),
            }
        }
        "shutdown_request" => {
            let teammate = input["teammate"].as_str().unwrap_or("");
            let req_id = state.protocols.create_shutdown_request(teammate);
            let _ = state
                .bus
                .send("lead", teammate, "Please shut down.", "shutdown_request");
            format!("Shutdown request {req_id} sent to '{teammate}'")
        }
        "plan_approval" => {
            let req_id = input["request_id"].as_str().unwrap_or("");
            let approve = input["approve"].as_bool().unwrap_or(false);
            let feedback = input["feedback"].as_str().unwrap_or("");
            match state.protocols.review_plan(req_id, approve, feedback) {
                Ok(r) => r,
                Err(e) => format!("Error: {e}"),
            }
        }
        "idle" => "Lead does not idle.".to_string(),
        "claim_task" => {
            let mut mgr = state.task_mgr.lock().unwrap();
            match mgr.update(
                input["task_id"].as_u64().unwrap_or(0) as u32,
                Some("in_progress"),
                None,
                None,
            ) {
                Ok(_) => format!("Claimed task #{}", input["task_id"]),
                Err(e) => format!("Error: {e}"),
            }
        }
        _ => format!("Unknown tool: {name}"),
    }
}

// Re-export for use in teammate module
pub use crate::agent_teams::MessageBus;
pub use crate::background_tasks::BackgroundManager as BgManager;
pub use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_client::AnthropicClient;
    use std::path::PathBuf;

    fn test_state() -> State {
        let client = AnthropicClient::new("test_key", "https://api.anthropic.com");
        let workdir = PathBuf::from("/tmp");
        State::new(client, "test-model".to_string(), workdir)
    }

    #[test]
    fn test_dispatch_unknown_tool() {
        let state = test_state();
        let input = serde_json::json!({});
        let result = dispatch_tool(&state, "unknown_tool", &input);
        assert!(result.contains("Unknown tool"));
    }

    #[test]
    fn test_dispatch_idle() {
        let state = test_state();
        let input = serde_json::json!({});
        let result = dispatch_tool(&state, "idle", &input);
        assert_eq!(result, "Lead does not idle.");
    }

    #[test]
    fn test_dispatch_compact() {
        let state = test_state();
        let input = serde_json::json!({});
        let result = dispatch_tool(&state, "compact", &input);
        assert_eq!(result, "Compacting...");
    }
}
