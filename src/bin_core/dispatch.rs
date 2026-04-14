use crate::bin_core::constants::LEAD;
use crate::bin_core::state::State;
use crate::llm_client::AnthropicClient;
use crate::tool_runners::{run_bash, run_edit, run_read, run_write, WorkdirRoot};
use serde_json::Value as Json;

/// Dispatch a tool call by name. Returns the output string.
pub async fn dispatch_tool(state: &State, name: &str, input: &Json) -> String {
    let wd = WorkdirRoot::new(&state.workdir).unwrap();
    match name {
        "bash" => run_bash(input["command"].as_str().unwrap_or(""), wd.as_path()),
        "read_file" => run_read(
            input["path"].as_str().unwrap_or(""),
            input["limit"].as_u64().map(|n| n as usize),
            &wd,
        ),
        "write_file" => run_write(
            input["path"].as_str().unwrap_or(""),
            input["content"].as_str().unwrap_or(""),
            &wd,
        ),
        "edit_file" => run_edit(
            input["path"].as_str().unwrap_or(""),
            input["old_text"].as_str().unwrap_or(""),
            input["new_text"].as_str().unwrap_or(""),
            &wd,
        ),
        "TodoWrite" => {
            let items = input["items"]
                .as_array()
                .map(|a| a.as_slice())
                .unwrap_or(&[]);
            let mut mgr = match state.todo.lock() {
                Ok(m) => m,
                Err(e) => return format!("Error: lock poisoned: {}", e),
            };
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
            tracing::info!(desc = %desc, prompt = %preview, "dispatching subagent task");

            // Run the subagent asynchronously using the existing runtime.
            state.subagent.run_subagent(prompt).await
        }
        "load_skill" => state
            .skills
            .get_content(input["name"].as_str().unwrap_or("")),
        "compact" => "Compacting...".to_string(),
        "background_run" => state
            .bg
            .run(input["command"].as_str().unwrap_or(""), wd.as_path()),
        "check_background" => state.bg.check(input["task_id"].as_str()),
        "task_create" => {
            let mut mgr = match state.task_mgr.lock() {
                Ok(m) => m,
                Err(e) => return format!("Error: lock poisoned: {}", e),
            };
            match mgr.create(
                input["subject"].as_str().unwrap_or(""),
                input["description"].as_str().unwrap_or(""),
            ) {
                Ok(r) => r,
                Err(e) => format!("Error: {e}"),
            }
        }
        "task_get" => {
            let mgr = match state.task_mgr.lock() {
                Ok(m) => m,
                Err(e) => return format!("Error: lock poisoned: {}", e),
            };
            match mgr.get(input["task_id"].as_u64().unwrap_or(0) as u32) {
                Ok(r) => r,
                Err(e) => format!("Error: {e}"),
            }
        }
        "task_update" => {
            let mut mgr = match state.task_mgr.lock() {
                Ok(m) => m,
                Err(e) => return format!("Error: lock poisoned: {}", e),
            };
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
            let mgr = match state.task_mgr.lock() {
                Ok(m) => m,
                Err(e) => return format!("Error: lock poisoned: {}", e),
            };
            mgr.list_all()
        }
        "spawn_teammate" => {
            let name = input["name"].as_str().unwrap_or("");
            let role = input["role"].as_str().unwrap_or("");
            let prompt = input["prompt"].as_str().unwrap_or("");
            let mut team = match state.team.lock() {
                Ok(t) => t,
                Err(e) => return format!("Error: lock poisoned: {}", e),
            };
            match team.spawn(name, role) {
                Ok(msg) => {
                    let client =
                        AnthropicClient::new(&state.client.api_key, &state.client.base_url);
                    let model = state.model.clone();
                    let workdir = state.workdir.clone();
                    let bus = Arc::clone(&state.bus);
                    let protocols = state.protocols.clone();
                    let task_mgr = Arc::clone(&state.task_mgr);
                    let name_owned = name.to_string();
                    let role_owned = role.to_string();
                    let prompt_owned = prompt.to_string();
                    let team_name = team.team_name().to_string();

                    tokio::spawn(async move {
                        crate::bin_core::teammate::teammate_loop(
                            client,
                            model,
                            workdir,
                            bus,
                            protocols,
                            task_mgr,
                            name_owned,
                            role_owned,
                            prompt_owned,
                            team_name,
                        )
                        .await
                    });
                    msg
                }
                Err(e) => format!("Error: {e}"),
            }
        }
        "list_teammates" => {
            let team = match state.team.lock() {
                Ok(t) => t,
                Err(e) => return format!("Error: lock poisoned: {}", e),
            };
            team.list_all()
        }
        "send_message" => {
            let to = input["to"].as_str().unwrap_or("");
            let content = input["content"].as_str().unwrap_or("");
            let msg_type = input["msg_type"].as_str().unwrap_or("message");
            match state.bus.send(LEAD, to, content, msg_type) {
                Ok(r) => r,
                Err(e) => format!("Error: {e}"),
            }
        }
        "read_inbox" => {
            let msgs = state.bus.read_inbox(LEAD);
            serde_json::to_string_pretty(&msgs).unwrap_or_default()
        }
        "broadcast" => {
            let team = match state.team.lock() {
                Ok(t) => t,
                Err(e) => return format!("Error: lock poisoned: {}", e),
            };
            let names = team.member_names();
            match state
                .bus
                .broadcast(LEAD, input["content"].as_str().unwrap_or(""), &names)
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
                .send(LEAD, teammate, "Please shut down.", "shutdown_request");
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
            let mut mgr = match state.task_mgr.lock() {
                Ok(m) => m,
                Err(e) => return format!("Error: lock poisoned: {}", e),
            };
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
        "worktree_create" => {
            let name = input["name"].as_str().unwrap_or("");
            let task_id = input["task_id"].as_u64().map(|n| n as u32);
            let base_ref = input["base_ref"].as_str().unwrap_or("HEAD");
            match state.worktree.create(name, task_id, base_ref) {
                Ok(r) => r,
                Err(e) => format!("Error: {e}"),
            }
        }
        "worktree_list" => match state.worktree.list_all() {
            Ok(r) => r,
            Err(e) => format!("Error: {e}"),
        },
        "worktree_remove" => {
            let name = input["name"].as_str().unwrap_or("");
            let force = input["force"].as_bool().unwrap_or(false);
            let complete_task = input["complete_task"].as_bool().unwrap_or(false);
            match state.worktree.remove(name, force, complete_task) {
                Ok(r) => r,
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
        State::new(client, "test-model".to_string(), workdir).unwrap()
    }

    #[tokio::test]
    async fn test_dispatch_unknown_tool() {
        let state = test_state();
        let input = serde_json::json!({});
        let result = dispatch_tool(&state, "unknown_tool", &input).await;
        assert!(result.contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_dispatch_idle() {
        let state = test_state();
        let input = serde_json::json!({});
        let result = dispatch_tool(&state, "idle", &input).await;
        assert_eq!(result, "Lead does not idle.");
    }

    #[tokio::test]
    async fn test_dispatch_compact() {
        let state = test_state();
        let input = serde_json::json!({});
        let result = dispatch_tool(&state, "compact", &input).await;
        assert_eq!(result, "Compacting...");
    }

    #[tokio::test]
    async fn test_dispatch_worktree_list() {
        let state = test_state();
        let input = serde_json::json!({});
        let result = dispatch_tool(&state, "worktree_list", &input).await;
        assert!(result.contains("No worktrees") || result.contains("["));
    }

    #[tokio::test]
    async fn test_dispatch_worktree_create_missing_git() {
        let state = test_state();
        let input = serde_json::json!({"name": "test-wt"});
        let result = dispatch_tool(&state, "worktree_create", &input).await;
        assert!(result.contains("Error:"));
    }
}
