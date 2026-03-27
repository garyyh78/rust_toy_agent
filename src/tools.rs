//! tools.rs - Tool definitions, TodoManager, and dispatch
//!
//! TOOLS constant (JSON schema), TodoManager, and the dispatch_tools router.
//! Path/file operations come from help_utils.

use crate::help_utils::{run_bash, run_edit, run_read, run_write};
use serde_json::Value as Json;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub const TOOLS: &str = r#"[{
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
pub struct TodoItem {
    pub id: String,
    pub text: String,
    pub status: String,
}

pub struct TodoManager {
    items: Vec<TodoItem>,
}

impl TodoManager {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn update(&mut self, items_json: &[Json]) -> Result<String, String> {
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

    pub fn render(&self) -> String {
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

/// Dispatch a tool call by name. Returns `(output, did_use_todo)`.
pub fn dispatch_tools(
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
            let items = input["items"]
                .as_array()
                .map(|a| a.as_slice())
                .unwrap_or(&[]);
            let mut mgr = todo.lock().unwrap();
            match mgr.update(items) {
                Ok(rendered) => (Some(rendered), true),
                Err(e) => (Some(format!("Error: {}", e)), true),
            }
        }
        _ => (None, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_tools_json_parsing() {
        let tools: Json = serde_json::from_str(TOOLS).unwrap();
        assert!(tools.is_array());
        let arr = tools.as_array().unwrap();
        assert_eq!(arr.len(), 5);

        let tool_names: Vec<&str> = arr.iter().map(|t| t["name"].as_str().unwrap()).collect();
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
    fn test_bash_tool_schema() {
        let tools: Json = serde_json::from_str(TOOLS).unwrap();
        let bash = &tools.as_array().unwrap()[0];
        assert_eq!(bash["name"], "bash");
        let schema = &bash["input_schema"];
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["command"].is_object());
    }

    #[test]
    fn test_todo_manager_basic() {
        let mut mgr = TodoManager::new();
        let items = vec![
            serde_json::json!({"id": "1", "text": "Write tests", "status": "pending"}),
            serde_json::json!({"id": "2", "text": "Run build", "status": "in_progress"}),
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
            serde_json::json!({"id": "1", "text": "Write tests", "status": "completed"}),
            serde_json::json!({"id": "2", "text": "Run build", "status": "completed"}),
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
            .map(|i| {
                serde_json::json!({"id": format!("{}", i), "text": format!("task {}", i), "status": "pending"})
            })
            .collect();
        let result = mgr.update(&items);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Max 20 todos"));
    }

    #[test]
    fn test_todo_manager_multiple_in_progress() {
        let mut mgr = TodoManager::new();
        let items = vec![
            serde_json::json!({"id": "1", "text": "Task A", "status": "in_progress"}),
            serde_json::json!({"id": "2", "text": "Task B", "status": "in_progress"}),
        ];
        let result = mgr.update(&items);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Only one task can be in_progress"));
    }

    #[test]
    fn test_todo_manager_empty_text() {
        let mut mgr = TodoManager::new();
        let items = vec![serde_json::json!({"id": "1", "text": "", "status": "pending"})];
        let result = mgr.update(&items);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("text required"));
    }

    #[test]
    fn test_todo_manager_invalid_status() {
        let mut mgr = TodoManager::new();
        let items = vec![serde_json::json!({"id": "1", "text": "Task", "status": "done"})];
        let result = mgr.update(&items);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid status"));
    }

    #[test]
    fn test_dispatch_todo_tool() {
        let todo = Arc::new(Mutex::new(TodoManager::new()));
        let input = serde_json::json!({
            "items": [{"id": "1", "text": "Test task", "status": "pending"}]
        });
        let (output, did_todo) = dispatch_tools("todo", &input, &PathBuf::from("."), &todo);
        assert!(did_todo);
        assert!(output.unwrap().contains("[ ] #1: Test task"));
    }

    #[test]
    fn test_dispatch_bash_not_todo() {
        let todo = Arc::new(Mutex::new(TodoManager::new()));
        let input = serde_json::json!({"command": "echo hello"});
        let (output, did_todo) = dispatch_tools("bash", &input, &PathBuf::from("."), &todo);
        assert!(!did_todo);
        assert!(output.unwrap().contains("hello"));
    }

    #[test]
    fn test_dispatch_unknown_tool() {
        let todo = Arc::new(Mutex::new(TodoManager::new()));
        let input = serde_json::json!({"foo": "bar"});
        let (output, did_todo) = dispatch_tools("unknown_tool", &input, &PathBuf::from("."), &todo);
        assert!(!did_todo);
        assert!(output.is_none());
    }

    #[test]
    fn test_tool_result_structure() {
        let result = serde_json::json!({
            "type": "tool_result",
            "tool_use_id": "abc123",
            "content": "output"
        });
        assert_eq!(result["type"], "tool_result");
        assert!(result["tool_use_id"].is_string());
        assert!(result["content"].is_string());
    }
}
