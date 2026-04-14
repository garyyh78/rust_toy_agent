use crate::agent_teams::{MessageBus, TeammateManager};
use crate::config::{
    IDLE_TIMEOUT_SECS, POLL_INTERVAL_SECS, TEAMMATE_MAX_ROUNDS, TEAMMATE_MAX_TOKENS,
};
use crate::context_compact::ContextCompactor;
use crate::llm_client::AnthropicClient;
use crate::task_system::TaskManager;
use crate::team_protocols::ProtocolTracker;
use crate::todo_manager::TodoManager;
use crate::tool_runners::{run_bash, run_edit, run_read, run_write, WorkdirRoot};
use crate::tools::teammate_tools;
use serde_json::Value as Json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[allow(clippy::too_many_arguments)]
pub async fn teammate_loop(
    client: AnthropicClient,
    model: String,
    workdir: PathBuf,
    bus: Arc<MessageBus>,
    protocols: ProtocolTracker,
    task_mgr: Arc<Mutex<TaskManager>>,
    name: String,
    role: String,
    prompt: String,
    team_name: String,
) {
    let _todo = TodoManager::new();
    let wd = match WorkdirRoot::new(&workdir) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!(error = %e, "failed to create workdir root");
            return;
        }
    };
    let compactor = ContextCompactor::new(
        AnthropicClient::new(&client.api_key, &client.base_url),
        workdir.to_string_lossy().to_string(),
        model.clone(),
    );

    let sys_prompt = format!(
        "You are '{name}', role: {role}, team: {team_name}, at {}. \
         Use idle when done with current work. You may auto-claim tasks.",
        workdir.display()
    );

    let team_tools = Json::Array(teammate_tools());

    let mut messages: Vec<Json> = vec![serde_json::json!({"role": "user", "content": prompt})];

    // WORK PHASE
    for _ in 0..TEAMMATE_MAX_ROUNDS {
        // Check inbox
        let inbox = bus.read_inbox(&name);
        for msg in &inbox {
            if msg.msg_type == "shutdown_request" {
                let _ = protocols.respond_shutdown(&msg.content, true);
                return;
            }
            messages.push(serde_json::json!({
                "role": "user",
                "content": serde_json::to_string(&msg).unwrap_or_default()
            }));
        }

        // Microcompact
        compactor.micro_compact(&mut messages);

        // LLM call
        let response = match client
            .create_message(
                &model,
                Some(&sys_prompt),
                &messages,
                Some(&team_tools),
                TEAMMATE_MAX_TOKENS,
            )
            .await
        {
            Ok(r) => r,
            Err(_) => return,
        };

        messages.push(serde_json::json!({
            "role": "assistant",
            "content": response["content"]
        }));

        if response["stop_reason"] != "tool_use" {
            break;
        }

        let mut results: Vec<Json> = Vec::new();
        let mut idle_requested = false;

        if let Some(content) = response["content"].as_array() {
            for block in content {
                if block["type"] == "tool_use" {
                    let tool_name = block["name"].as_str().unwrap_or("");
                    let input = &block["input"];

                    let output = match tool_name {
                        "idle" => {
                            idle_requested = true;
                            "Entering idle phase.".to_string()
                        }
                        "claim_task" => {
                            let tid = input["task_id"].as_u64().unwrap_or(0) as u32;
                            match task_mgr.lock().unwrap().update(
                                tid,
                                Some("in_progress"),
                                None,
                                None,
                            ) {
                                Ok(_) => format!("Claimed task #{tid}"),
                                Err(e) => format!("Error: {e}"),
                            }
                        }
                        "send_message" => {
                            let to = input["to"].as_str().unwrap_or("");
                            let content = input["content"].as_str().unwrap_or("");
                            match bus.send(&name, to, content, "message") {
                                Ok(r) => r,
                                Err(e) => format!("Error: {e}"),
                            }
                        }
                        "bash" => run_bash(input["command"].as_str().unwrap_or(""), &workdir),
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
                        _ => format!("Unknown tool: {tool_name}"),
                    };

                    let preview = if output.chars().count() > 120 {
                        format!("{}...", output.chars().take(120).collect::<String>())
                    } else {
                        output.clone()
                    };
                    tracing::info!(name = %name, tool = %tool_name, output = %preview, "teammate tool");
                    results.push(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": block["id"],
                        "content": output
                    }));
                }
            }
        }

        messages.push(serde_json::json!({
            "role": "user",
            "content": results
        }));

        if idle_requested {
            break;
        }
    }

    // IDLE PHASE: poll for messages and unclaimed tasks
    let Ok(team) = TeammateManager::new(&workdir.join(".team")) else {
        return;
    };
    let mut team = team;
    team.set_status(&name, "idle");

    let idle_polls = (IDLE_TIMEOUT_SECS / POLL_INTERVAL_SECS.max(1)) as usize;
    let mut resume = false;

    for _ in 0..idle_polls {
        tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;

        let inbox = bus.read_inbox(&name);
        if !inbox.is_empty() {
            for msg in &inbox {
                if msg.msg_type == "shutdown_request" {
                    team.set_status(&name, "shutdown");
                    return;
                }
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": serde_json::to_string(&msg).unwrap_or_default()
                }));
            }
            resume = true;
            break;
        }

        // Auto-claim unclaimed tasks
        {
            let list = task_mgr.lock().unwrap().list_all();
            let unclaimed_tid: Option<u32> = list
                .lines()
                .filter(|l| l.contains("[ ]") && !l.contains("blocked"))
                .find_map(|line| {
                    line.split('#')
                        .nth(1)
                        .and_then(|s| s.split(':').next())
                        .and_then(|s| s.trim().parse::<u32>().ok())
                });
            if let Some(tid) = unclaimed_tid {
                if task_mgr
                    .lock()
                    .unwrap()
                    .update(tid, Some("in_progress"), None, None)
                    .is_ok()
                {
                    // Identity re-injection for compressed contexts
                    if messages.len() <= 3 {
                        messages.insert(
                            0,
                            serde_json::json!({"role": "user", "content":
                                format!("<identity>You are '{name}', role: {role}, team: {team_name}.</identity>")}),
                        );
                        messages.insert(
                            1,
                            serde_json::json!({"role": "assistant", "content":
                                format!("I am {name}. Continuing.")}),
                        );
                    }
                    messages.push(serde_json::json!({"role": "user", "content":
                        format!("<auto-claimed>Task #{tid} from the board.")}));
                    messages.push(serde_json::json!({"role": "assistant", "content":
                        format!("Claimed task #{tid}. Working on it.")}));
                    resume = true;
                    break;
                }
            }
        }
    }

    if !resume {
        team.set_status(&name, "shutdown");
        return;
    }

    team.set_status(&name, "working");
    // Could recursively call teammate_loop here to continue work
}
