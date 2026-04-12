//! main.rs - Full Agent (modelled after s_full.py)
//!
//! Capstone agent combining every mechanism from the library modules.
//!
//!   +------------------------------------------------------------------+
//!   |                        FULL AGENT                                 |
//!   |                                                                   |
//!   |  System prompt (skills, task-first + optional todo nag)          |
//!   |                                                                   |
//!   |  Before each LLM call:                                            |
//!   |  +--------------------+  +------------------+  +--------------+  |
//!   |  | Microcompact       |  | Drain bg         |  | Check inbox  |  |
//!   |  | Auto-compact       |  | notifications    |  |              |  |
//!   |  +--------------------+  +------------------+  +--------------+  |
//!   |                                                                   |
//!   |  Tool dispatch: bash, read_file, write_file, edit_file,          |
//!   |    TodoWrite, task, load_skill, compact, background_run,         |
//!   |    check_background, task_create, task_get, task_update,         |
//!   |    task_list, spawn_teammate, list_teammates, send_message,      |
//!   |    read_inbox, broadcast, shutdown_request, plan_approval,       |
//!   |    idle, claim_task                                               |
//!   |                                                                   |
//!   |  Subagent:  spawn -> work -> return summary                      |
//!   |  Teammate:  spawn -> work -> idle -> auto-claim                  |
//!   |  Shutdown:  request_id handshake                                  |
//!   |  Plan gate: submit -> approve/reject                              |
//!   +------------------------------------------------------------------+
//!
//!   REPL commands: /compact /tasks /team /inbox
//!
//! Modules used:
//!   llm_client, tool_runners, todo_manager, task_system,
//!   background_tasks, agent_teams, skill_loading, subagent,
//!   context_compact, team_protocols, e2e_test

use rust_toy_agent::agent_teams::{MessageBus, TeammateManager};
use rust_toy_agent::background_tasks::BackgroundManager;
use rust_toy_agent::context_compact::ContextCompactor;
use rust_toy_agent::e2e_test::{load_test_case, print_test_result, run_test, save_test_result};
use rust_toy_agent::llm_client::AnthropicClient;
use rust_toy_agent::skill_loading::SkillLoader;
use rust_toy_agent::subagent::Subagent;
use rust_toy_agent::task_system::TaskManager;
use rust_toy_agent::team_protocols::ProtocolTracker;
use rust_toy_agent::todo_manager::TodoManager;
use rust_toy_agent::tool_runners::{run_bash, run_edit, run_read, run_write};

use serde_json::Value as Json;
use std::env;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// ── Constants ──────────────────────────────────────────────────────────

const TOKEN_THRESHOLD: usize = 100_000;
const POLL_INTERVAL: u64 = 5;
const IDLE_TIMEOUT: u64 = 60;
const MAX_TOKENS: u32 = 8_000;

// ── Full Tool Definitions ──────────────────────────────────────────────

const FULL_TOOLS: &str = r#"[
  {
    "name": "bash",
    "description": "Run a shell command.",
    "input_schema": {
      "type": "object",
      "properties": {"command": {"type": "string"}},
      "required": ["command"]
    }
  },
  {
    "name": "read_file",
    "description": "Read file contents.",
    "input_schema": {
      "type": "object",
      "properties": {"path": {"type": "string"}, "limit": {"type": "integer"}},
      "required": ["path"]
    }
  },
  {
    "name": "write_file",
    "description": "Write content to file.",
    "input_schema": {
      "type": "object",
      "properties": {"path": {"type": "string"}, "content": {"type": "string"}},
      "required": ["path", "content"]
    }
  },
  {
    "name": "edit_file",
    "description": "Replace exact text in file.",
    "input_schema": {
      "type": "object",
      "properties": {"path": {"type": "string"}, "old_text": {"type": "string"}, "new_text": {"type": "string"}},
      "required": ["path", "old_text", "new_text"]
    }
  },
  {
    "name": "TodoWrite",
    "description": "Update task tracking list.",
    "input_schema": {
      "type": "object",
      "properties": {"items": {"type": "array", "items": {"type": "object", "properties": {"content": {"type": "string"}, "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]}}}, "required": ["content", "status"]}},
      "required": ["items"]
    }
  },
  {
    "name": "task",
    "description": "Spawn a subagent for isolated exploration or work.",
    "input_schema": {
      "type": "object",
      "properties": {"prompt": {"type": "string"}, "description": {"type": "string"}},
      "required": ["prompt"]
    }
  },
  {
    "name": "load_skill",
    "description": "Load specialized knowledge by name.",
    "input_schema": {
      "type": "object",
      "properties": {"name": {"type": "string"}},
      "required": ["name"]
    }
  },
  {
    "name": "compact",
    "description": "Manually compact conversation context.",
    "input_schema": {"type": "object", "properties": {}}
  },
  {
    "name": "background_run",
    "description": "Run command in background thread.",
    "input_schema": {
      "type": "object",
      "properties": {"command": {"type": "string"}, "timeout": {"type": "integer"}},
      "required": ["command"]
    }
  },
  {
    "name": "check_background",
    "description": "Check background task status.",
    "input_schema": {
      "type": "object",
      "properties": {"task_id": {"type": "string"}}
    }
  },
  {
    "name": "task_create",
    "description": "Create a persistent file task.",
    "input_schema": {
      "type": "object",
      "properties": {"subject": {"type": "string"}, "description": {"type": "string"}},
      "required": ["subject"]
    }
  },
  {
    "name": "task_get",
    "description": "Get task details by ID.",
    "input_schema": {
      "type": "object",
      "properties": {"task_id": {"type": "integer"}},
      "required": ["task_id"]
    }
  },
  {
    "name": "task_update",
    "description": "Update task status or dependencies.",
    "input_schema": {
      "type": "object",
      "properties": {"task_id": {"type": "integer"}, "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]}, "add_blocked_by": {"type": "array", "items": {"type": "integer"}}, "add_blocks": {"type": "array", "items": {"type": "integer"}}},
      "required": ["task_id"]
    }
  },
  {
    "name": "task_list",
    "description": "List all tasks.",
    "input_schema": {"type": "object", "properties": {}}
  },
  {
    "name": "spawn_teammate",
    "description": "Spawn a persistent autonomous teammate.",
    "input_schema": {
      "type": "object",
      "properties": {"name": {"type": "string"}, "role": {"type": "string"}, "prompt": {"type": "string"}},
      "required": ["name", "role", "prompt"]
    }
  },
  {
    "name": "list_teammates",
    "description": "List all teammates.",
    "input_schema": {"type": "object", "properties": {}}
  },
  {
    "name": "send_message",
    "description": "Send a message to a teammate.",
    "input_schema": {
      "type": "object",
      "properties": {"to": {"type": "string"}, "content": {"type": "string"}, "msg_type": {"type": "string", "enum": ["message", "broadcast", "shutdown_request", "shutdown_response", "plan_approval_response"]}},
      "required": ["to", "content"]
    }
  },
  {
    "name": "read_inbox",
    "description": "Read and drain the lead's inbox.",
    "input_schema": {"type": "object", "properties": {}}
  },
  {
    "name": "broadcast",
    "description": "Send message to all teammates.",
    "input_schema": {
      "type": "object",
      "properties": {"content": {"type": "string"}},
      "required": ["content"]
    }
  },
  {
    "name": "shutdown_request",
    "description": "Request a teammate to shut down.",
    "input_schema": {
      "type": "object",
      "properties": {"teammate": {"type": "string"}},
      "required": ["teammate"]
    }
  },
  {
    "name": "plan_approval",
    "description": "Approve or reject a teammate's plan.",
    "input_schema": {
      "type": "object",
      "properties": {"request_id": {"type": "string"}, "approve": {"type": "boolean"}, "feedback": {"type": "string"}},
      "required": ["request_id", "approve"]
    }
  },
  {
    "name": "idle",
    "description": "Enter idle state.",
    "input_schema": {"type": "object", "properties": {}}
  },
  {
    "name": "claim_task",
    "description": "Claim a task from the board.",
    "input_schema": {
      "type": "object",
      "properties": {"task_id": {"type": "integer"}},
      "required": ["task_id"]
    }
  }
]"#;

// ── Agent State ────────────────────────────────────────────────────────

struct State {
    client: AnthropicClient,
    model: String,
    workdir: PathBuf,
    todo: Mutex<TodoManager>,
    task_mgr: Mutex<TaskManager>,
    bg: BackgroundManager,
    bus: Arc<MessageBus>,
    team: Mutex<TeammateManager>,
    skills: SkillLoader,
    protocols: ProtocolTracker,
    compactor: ContextCompactor,
    subagent: Subagent,
}

impl State {
    fn new(client: AnthropicClient, model: String, workdir: PathBuf) -> Self {
        let skills_dir = workdir.join("skills").to_string_lossy().to_string();
        let skills = SkillLoader::new(&skills_dir);
        let task_mgr = TaskManager::new(&workdir.join(".tasks")).expect("Failed to create .tasks");
        let team = TeammateManager::new(&workdir.join(".team")).expect("Failed to create .team");
        let compactor = ContextCompactor::new(
            AnthropicClient::new(&client.api_key, &client.base_url),
            workdir.to_string_lossy().to_string(),
            model.clone(),
        );
        let subagent = Subagent::new(
            AnthropicClient::new(&client.api_key, &client.base_url),
            workdir.to_string_lossy().to_string(),
            model.clone(),
        );

        Self {
            client,
            model,
            workdir,
            todo: Mutex::new(TodoManager::new()),
            task_mgr: Mutex::new(task_mgr),
            bg: BackgroundManager::new(),
            bus: Arc::new(MessageBus::new()),
            team: Mutex::new(team),
            skills,
            protocols: ProtocolTracker::new(),
            compactor,
            subagent,
        }
    }
}

// ── Tool Dispatch ──────────────────────────────────────────────────────

fn dispatch_tool(state: &State, name: &str, input: &Json) -> String {
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
            eprintln!("  > task ({desc}): {}", &prompt[..prompt.len().min(80)]);
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

                    thread::spawn(move || {
                        teammate_loop(
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

// ── Main Agent Loop ────────────────────────────────────────────────────

async fn agent_loop(state: &State, messages: &mut Vec<Json>, system: &str) {
    let tools: Json = serde_json::from_str(FULL_TOOLS).unwrap();
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
                    format!(
                        "[bg:{}] {}: {}",
                        n.task_id,
                        n.status,
                        &n.result[..n.result.len().min(500)]
                    )
                })
                .collect();
            messages.push(serde_json::json!({
                "role": "user",
                "content": format!("<background-results>\n{}\n</background-results>", txt.join("\n"))
            }));
        }

        // Check lead inbox
        let inbox = state.bus.read_inbox("lead");
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
                    eprintln!("{}", &output[..output.len().min(200)]);

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
        if has_open && rounds_since_todo >= 3 {
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

// ── Teammate Loop ──────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn teammate_loop(
    client: AnthropicClient,
    model: String,
    workdir: PathBuf,
    bus: Arc<MessageBus>,
    protocols: ProtocolTracker,
    name: &str,
    role: &str,
    prompt: &str,
    team_name: &str,
) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _todo = TodoManager::new();
    let mut task_mgr = match TaskManager::new(&workdir.join(".tasks")) {
        Ok(m) => m,
        Err(_) => return,
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

    let team_tools: Json = serde_json::from_str(r#"[
      {
        "name": "bash",
        "description": "Run a shell command.",
        "input_schema": {
          "type": "object",
          "properties": {"command": {"type": "string"}},
          "required": ["command"]
        }
      },
      {
        "name": "read_file",
        "description": "Read file contents.",
        "input_schema": {
          "type": "object",
          "properties": {"path": {"type": "string"}, "limit": {"type": "integer"}},
          "required": ["path"]
        }
      },
      {
        "name": "write_file",
        "description": "Write content to file.",
        "input_schema": {
          "type": "object",
          "properties": {"path": {"type": "string"}, "content": {"type": "string"}},
          "required": ["path", "content"]
        }
      },
      {
        "name": "edit_file",
        "description": "Replace exact text in file.",
        "input_schema": {
          "type": "object",
          "properties": {"path": {"type": "string"}, "old_text": {"type": "string"}, "new_text": {"type": "string"}},
          "required": ["path", "old_text", "new_text"]
        }
      },
      {
        "name": "send_message",
        "description": "Send a message to a teammate.",
        "input_schema": {
          "type": "object",
          "properties": {"to": {"type": "string"}, "content": {"type": "string"}},
          "required": ["to", "content"]
        }
      },
      {
        "name": "idle",
        "description": "Signal no more work.",
        "input_schema": {"type": "object", "properties": {}}
      },
      {
        "name": "claim_task",
        "description": "Claim task by ID.",
        "input_schema": {
          "type": "object",
          "properties": {"task_id": {"type": "integer"}},
          "required": ["task_id"]
        }
      }
    ]"#).unwrap();

    let mut messages: Vec<Json> = vec![serde_json::json!({"role": "user", "content": prompt})];

    // WORK PHASE
    for _ in 0..50 {
        // Check inbox
        let inbox = bus.read_inbox(name);
        for msg in &inbox {
            if msg.msg_type == "shutdown_request" {
                let _ = protocols.respond_shutdown(
                    &msg.content, // request_id is in content
                    true,
                );
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
        let response = match rt.block_on(client.create_message(
            &model,
            Some(&sys_prompt),
            &messages,
            Some(&team_tools),
            8000,
        )) {
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
                            match task_mgr.update(tid, Some("in_progress"), None, None) {
                                Ok(_) => format!("Claimed task #{tid}"),
                                Err(e) => format!("Error: {e}"),
                            }
                        }
                        "send_message" => {
                            let to = input["to"].as_str().unwrap_or("");
                            let content = input["content"].as_str().unwrap_or("");
                            match bus.send(name, to, content, "message") {
                                Ok(r) => r,
                                Err(e) => format!("Error: {e}"),
                            }
                        }
                        "bash" => run_bash(input["command"].as_str().unwrap_or(""), &workdir),
                        "read_file" => run_read(
                            input["path"].as_str().unwrap_or(""),
                            input["limit"].as_u64().map(|n| n as usize),
                            &workdir,
                        ),
                        "write_file" => run_write(
                            input["path"].as_str().unwrap_or(""),
                            input["content"].as_str().unwrap_or(""),
                            &workdir,
                        ),
                        "edit_file" => run_edit(
                            input["path"].as_str().unwrap_or(""),
                            input["old_text"].as_str().unwrap_or(""),
                            input["new_text"].as_str().unwrap_or(""),
                            &workdir,
                        ),
                        _ => format!("Unknown tool: {tool_name}"),
                    };

                    eprintln!(
                        "  [{name}] {tool_name}: {}",
                        &output[..output.len().min(120)]
                    );
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
    let mut team = match TeammateManager::new(&workdir.join(".team")) {
        Ok(t) => t,
        Err(_) => return,
    };
    team.set_status(name, "idle");

    let idle_polls = (IDLE_TIMEOUT / POLL_INTERVAL.max(1)) as usize;
    let mut resume = false;

    for _ in 0..idle_polls {
        thread::sleep(Duration::from_secs(POLL_INTERVAL));

        let inbox = bus.read_inbox(name);
        if !inbox.is_empty() {
            for msg in &inbox {
                if msg.msg_type == "shutdown_request" {
                    team.set_status(name, "shutdown");
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
            let list = task_mgr.list_all();
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
        team.set_status(name, "shutdown");
        return;
    }

    team.set_status(name, "working");
    // Could recursively call teammate_loop here to continue work
}

// ── REPL ───────────────────────────────────────────────────────────────

fn read_prompt(prompt: &str) -> Option<String> {
    print!("{prompt}");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim().to_string()),
    }
}

fn extract_final_text(messages: &[Json]) -> String {
    let mut text = String::new();
    if let Some(last) = messages.last() {
        if let Some(blocks) = last["content"].as_array() {
            for block in blocks {
                if block["type"] == "text" {
                    if let Some(t) = block["text"].as_str() {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(t);
                    }
                }
            }
        }
    }
    text
}

fn print_usage() {
    eprintln!("Usage: rust_toy_agent [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --test <test_name>    Run in end2end test mode with the specified test");
    eprintln!("  -h, --help            Show this help message");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  rust_toy_agent                    # Start in interactive REPL mode");
    eprintln!("  rust_toy_agent --test pi_series   # Run the pi_series test");
}

async fn run_test_mode(test_name: &str) {
    let workdir = env::current_dir().unwrap();
    let test_path = workdir.join("task_tests").join(test_name).join("test.json");
    let results_dir = workdir.join("task_tests").join("test_results");

    if !test_path.exists() {
        eprintln!(
            "Error: Test '{test_name}' not found at {}",
            test_path.display()
        );
        std::process::exit(1);
    }

    let test_case = match load_test_case(&test_path) {
        Ok(tc) => tc,
        Err(e) => {
            eprintln!("Error loading test: {e}");
            std::process::exit(1);
        }
    };

    eprintln!();
    eprintln!("\x1b[35m╔══════════════════════════════════════════════════════════════╗\x1b[0m");
    eprintln!("\x1b[35m║          End-to-End Test Mode                              ║\x1b[0m");
    eprintln!("\x1b[35m╚══════════════════════════════════════════════════════════════╝\x1b[0m");
    eprintln!();
    eprintln!("  Test: {}", test_case.name);
    eprintln!("  Path: {}", test_path.display());
    eprintln!();

    let client = AnthropicClient::from_env();
    let model = env::var("MODEL_ID").expect("MODEL_ID not set");

    let todo = Arc::new(Mutex::new(TodoManager::new()));

    let test_workdir = workdir.join("task_tests").join(test_name).join("workspace");
    std::fs::remove_dir_all(&test_workdir).ok();
    std::fs::create_dir_all(&test_workdir).ok();

    // Use a no-op logger for test mode
    let mut logger = rust_toy_agent::logger::SessionLogger::stderr_only();

    let result = run_test(
        &client,
        &model,
        &test_case,
        &test_workdir,
        &todo,
        &mut logger,
    )
    .await;

    if let Err(e) = save_test_result(&result, &results_dir) {
        eprintln!("Error saving test result: {e}");
    } else {
        println!(
            "  Result saved to: {}",
            results_dir
                .join(format!("{}_{}.json", result.name, result.test_time))
                .display()
        );
    }

    print_test_result(&result);

    if result.passed {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}

async fn run_repl() {
    dotenvy::dotenv().ok();

    let workdir = env::current_dir().unwrap();
    let client = AnthropicClient::from_env();
    let model = env::var("MODEL_ID").expect("MODEL_ID not set");

    let state = State::new(client, model.clone(), workdir.clone());

    let system = format!(
        "You are a coding agent at {}. \
         Use tools to solve tasks. Prefer task_create/task_update/task_list for multi-step work. \
         Use TodoWrite for short checklists. Use task for subagent delegation. \
         Use load_skill for specialized knowledge.\n\
         Skills: {}",
        workdir.display(),
        state.skills.get_descriptions()
    );

    eprintln!();
    eprintln!("\x1b[35m╔══════════════════════════════════════════════════════════════╗\x1b[0m");
    eprintln!("\x1b[35m║          Full Agent - All Mechanisms Edition                ║\x1b[0m");
    eprintln!("\x1b[35m╚══════════════════════════════════════════════════════════════╝\x1b[0m");
    eprintln!();
    eprintln!("  Model: {model}");
    eprintln!("  Workdir: {}", workdir.display());
    eprintln!("  Tools: 23 (bash, read, write, edit, TodoWrite, task, load_skill,");
    eprintln!("           compact, bg_run, bg_check, task CRUD, team, messaging,");
    eprintln!("           broadcast, shutdown, plan, idle, claim)");
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    eprintln!();

    let mut history: Vec<Json> = Vec::new();

    loop {
        let query = match read_prompt("\x1b[36mfull >> \x1b[0m") {
            None => break,
            Some(q) => q,
        };
        if matches!(query.as_str(), "q" | "exit" | "") {
            eprintln!("\x1b[35m  Session ended.\x1b[0m");
            break;
        }

        match query.as_str() {
            "/compact" => {
                if !history.is_empty() {
                    eprintln!("[manual compact via /compact]");
                    history = state.compactor.auto_compact(&history).await;
                }
                continue;
            }
            "/tasks" => {
                let mgr = state.task_mgr.lock().unwrap();
                println!("{}", mgr.list_all());
                continue;
            }
            "/team" => {
                let team = state.team.lock().unwrap();
                println!("{}", team.list_all());
                continue;
            }
            "/inbox" => {
                let msgs = state.bus.read_inbox("lead");
                println!(
                    "{}",
                    serde_json::to_string_pretty(&msgs).unwrap_or_default()
                );
                continue;
            }
            _ => {}
        }

        eprintln!();
        eprintln!("\x1b[35m  Turn {}\x1b[0m", &query[..query.len().min(50)]);
        eprintln!();

        history.push(serde_json::json!({"role": "user", "content": query}));
        agent_loop(&state, &mut history, &system).await;

        let response_text = extract_final_text(&history);
        println!("{response_text}");
        println!();
    }
}

// ── Main ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return;
    }

    if let Some(idx) = args.iter().position(|a| a == "--test") {
        if let Some(test_name) = args.get(idx + 1) {
            run_test_mode(test_name).await;
            return;
        } else {
            eprintln!("Error: --test requires a test name argument");
            print_usage();
            std::process::exit(1);
        }
    }

    run_repl().await;
}
