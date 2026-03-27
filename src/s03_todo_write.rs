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

use serde_json::json;
use serde_json::Value as Json;
use std::env;
use std::io::{BufRead, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command as Proc;
use std::sync::{Arc, Mutex};

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
}, {
    "name": "todo",
    "description": "Update task list. Track progress on multi-step tasks.",
    "input_schema": {
        "type": "object",
        "properties": {
            "items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {"type": "string"},
                        "text": {"type": "string"},
                        "status": {
                            "type": "string",
                            "enum": ["pending", "in_progress", "completed"]
                        }
                    },
                    "required": ["id", "text", "status"]
                }
            }
        },
        "required": ["items"]
    }
}]"#;

// -- TodoManager --

#[derive(Debug, Clone)]
struct TodoItem {
    id: String,
    text: String,
    status: String,
}

struct TodoManager {
    items: Vec<TodoItem>,
}

impl TodoManager {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    fn update(&mut self, items_json: &[Json]) -> Result<String, String> {
        if items_json.len() > 20 {
            return Err("Max 20 todos allowed".to_string());
        }
        let mut validated = Vec::new();
        let mut in_progress_count = 0usize;
        for (i, item) in items_json.iter().enumerate() {
            let text = item
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let status = item
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("pending")
                .to_lowercase();
            let item_id = item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or(&format!("{}", i + 1))
                .to_string();
            if text.is_empty() {
                return Err(format!("Item {}: text required", item_id));
            }
            if !matches!(status.as_str(), "pending" | "in_progress" | "completed") {
                return Err(format!("Item {}: invalid status '{}'", item_id, status));
            }
            if status == "in_progress" {
                in_progress_count += 1;
            }
            validated.push(TodoItem {
                id: item_id,
                text,
                status,
            });
        }
        if in_progress_count > 1 {
            return Err("Only one task can be in_progress at a time".to_string());
        }
        self.items = validated;
        Ok(self.render())
    }

    fn render(&self) -> String {
        if self.items.is_empty() {
            return "No todos.".to_string();
        }
        let mut lines = Vec::new();
        for item in &self.items {
            let marker = match item.status.as_str() {
                "in_progress" => "[>]",
                "completed" => "[x]",
                _ => "[ ]",
            };
            lines.push(format!("{} #{}: {}", marker, item.id, item.text));
        }
        let done = self
            .items
            .iter()
            .filter(|t| t.status == "completed")
            .count();
        lines.push(format!("\n({}/{} completed)", done, self.items.len()));
        lines.join("\n")
    }
}

// -- AnthropicClient --

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

// -- Path helpers --

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

// -- Tool implementations --

fn run_bash(command: &str, workdir: &Path) -> String {
    let blocked = ["rm -rf /", "sudo", "shutdown", "reboot", "> /dev/"];
    if blocked.iter().any(|b| command.contains(b)) {
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

fn dispatch_tools(
    tool_name: &str,
    input: &Json,
    workdir: &Path,
    todo: &Arc<Mutex<TodoManager>>,
) -> (Option<String>, bool) {
    match tool_name {
        "bash" => (
            Some(run_bash(input["command"].as_str().unwrap_or(""), workdir)),
            false,
        ),
        "read_file" => (
            Some(run_read(
                input["path"].as_str().unwrap_or(""),
                input["limit"].as_u64().map(|n| n as usize),
                workdir,
            )),
            false,
        ),
        "write_file" => (
            Some(run_write(
                input["path"].as_str().unwrap_or(""),
                input["content"].as_str().unwrap_or(""),
                workdir,
            )),
            false,
        ),
        "edit_file" => (
            Some(run_edit(
                input["path"].as_str().unwrap_or(""),
                input["old_text"].as_str().unwrap_or(""),
                input["new_text"].as_str().unwrap_or(""),
                workdir,
            )),
            false,
        ),
        "todo" => {
            let items = input["items"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
            let mut mgr = todo.lock().unwrap();
            match mgr.update(items) {
                Ok(rendered) => (Some(rendered), true),
                Err(e) => (Some(format!("Error: {}", e)), true),
            }
        }
        _ => (None, false),
    }
}

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
    fn test_tools_json_parsing() {
        let tools: Json = serde_json::from_str(TOOLS).unwrap();
        assert!(tools.is_array());
        let arr = tools.as_array().unwrap();
        assert_eq!(arr.len(), 5);

        let tool_names: Vec<&str> = arr
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            tool_names,
            vec!["bash", "read_file", "write_file", "edit_file", "todo"]
        );
    }

    #[test]
    fn test_todo_tool_schema() {
        let tools: Json = serde_json::from_str(TOOLS).unwrap();
        let todo_tool = &tools.as_array().unwrap()[4];
        assert_eq!(todo_tool["name"], "todo");
        let schema = &todo_tool["input_schema"];
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["items"].is_object());
        let items_schema = &schema["properties"]["items"];
        assert_eq!(items_schema["type"], "array");
        let item_props = &items_schema["items"]["properties"];
        assert!(item_props["id"].is_object());
        assert!(item_props["text"].is_object());
        assert!(item_props["status"].is_object());
        let status_enum = &item_props["status"]["enum"];
        assert!(status_enum.is_array());
        let enums: Vec<&str> = status_enum
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(enums, vec!["pending", "in_progress", "completed"]);
    }

    #[test]
    fn test_todo_manager_basic() {
        let mut mgr = TodoManager::new();
        let items = vec![
            json!({"id": "1", "text": "Write tests", "status": "pending"}),
            json!({"id": "2", "text": "Run build", "status": "in_progress"}),
        ];
        let result = mgr.update(&items).unwrap();
        assert!(result.contains("[ ] #1: Write tests"));
        assert!(result.contains("[>] #2: Run build"));
        assert!(result.contains("(0/2 completed)"));
    }

    #[test]
    fn test_todo_manager_completed() {
        let mut mgr = TodoManager::new();
        let items = vec![
            json!({"id": "1", "text": "Write tests", "status": "completed"}),
            json!({"id": "2", "text": "Run build", "status": "completed"}),
        ];
        let result = mgr.update(&items).unwrap();
        assert!(result.contains("[x] #1: Write tests"));
        assert!(result.contains("[x] #2: Run build"));
        assert!(result.contains("(2/2 completed)"));
    }

    #[test]
    fn test_todo_manager_empty() {
        let mgr = TodoManager::new();
        assert_eq!(mgr.render(), "No todos.");
    }

    #[test]
    fn test_todo_manager_max_items() {
        let mut mgr = TodoManager::new();
        let items: Vec<Json> = (1..=21)
            .map(|i| json!({"id": format!("{}", i), "text": format!("task {}", i), "status": "pending"}))
            .collect();
        let result = mgr.update(&items);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Max 20 todos"));
    }

    #[test]
    fn test_todo_manager_multiple_in_progress() {
        let mut mgr = TodoManager::new();
        let items = vec![
            json!({"id": "1", "text": "Task A", "status": "in_progress"}),
            json!({"id": "2", "text": "Task B", "status": "in_progress"}),
        ];
        let result = mgr.update(&items);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Only one task can be in_progress"));
    }

    #[test]
    fn test_todo_manager_empty_text() {
        let mut mgr = TodoManager::new();
        let items = vec![json!({"id": "1", "text": "", "status": "pending"})];
        let result = mgr.update(&items);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("text required"));
    }

    #[test]
    fn test_todo_manager_invalid_status() {
        let mut mgr = TodoManager::new();
        let items = vec![json!({"id": "1", "text": "Task", "status": "done"})];
        let result = mgr.update(&items);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid status"));
    }

    #[test]
    fn test_bash_tool_still_works() {
        let tools: Json = serde_json::from_str(TOOLS).unwrap();
        let bash = &tools.as_array().unwrap()[0];
        assert_eq!(bash["name"], "bash");
        let schema = &bash["input_schema"];
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["command"].is_object());
    }

    #[test]
    fn test_dispatch_todo_tool() {
        let todo = Arc::new(Mutex::new(TodoManager::new()));
        let input = json!({
            "items": [{"id": "1", "text": "Test task", "status": "pending"}]
        });
        let (output, did_todo) = dispatch_tools("todo", &input, &PathBuf::from("."), &todo);
        assert!(did_todo);
        assert!(output.unwrap().contains("[ ] #1: Test task"));
    }

    #[test]
    fn test_dispatch_bash_not_todo() {
        let todo = Arc::new(Mutex::new(TodoManager::new()));
        let input = json!({"command": "echo hello"});
        let (output, did_todo) = dispatch_tools("bash", &input, &PathBuf::from("."), &todo);
        assert!(!did_todo);
        assert!(output.unwrap().contains("hello"));
    }

    #[test]
    fn test_dispatch_unknown_tool() {
        let todo = Arc::new(Mutex::new(TodoManager::new()));
        let input = json!({"foo": "bar"});
        let (output, did_todo) =
            dispatch_tools("unknown_tool", &input, &PathBuf::from("."), &todo);
        assert!(!did_todo);
        assert!(output.is_none());
    }

    #[test]
    fn test_nag_reminder_threshold() {
        let mut rounds_since_todo = 0usize;
        // Simulate 2 rounds without todo - no reminder yet
        rounds_since_todo += 1;
        assert!(rounds_since_todo < 3);
        rounds_since_todo += 1;
        assert!(rounds_since_todo < 3);
        // 3rd round - reminder triggers
        rounds_since_todo += 1;
        assert!(rounds_since_todo >= 3);
        // After todo use, resets to 0
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
        let workdir = PathBuf::from("/test/path");
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
