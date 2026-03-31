//! tools.rs - Tool definitions and dispatch
//!
//! TOOLS constant (JSON schema) and the dispatch_tools router.
//! Path/file operations come from tool_runners, todo state from todo_manager.
//!
//! ┌──────────────────────────────────────────────────────────────┐
//! │                        tools.rs                              │
//! ├──────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  TOOLS ── JSON schema sent to Anthropic API                  │
//! │    ├── bash        ── run_bash()      [tool_runners]         │
//! │    ├── read_file   ── run_read()      [tool_runners]         │
//! │    ├── write_file  ── run_write()     [tool_runners]         │
//! │    ├── edit_file   ── run_edit()      [tool_runners]         │
//! │    └── todo        ── TodoManager.update() [todo_manager]    │
//! │                                                              │
//! │  dispatch_tools(name, input, workdir, todo)                  │
//! │    │                                                         │
//! │    ├── "bash"       ──→ run_bash()    ──→ (output, false)    │
//! │    ├── "read_file"  ──→ run_read()    ──→ (output, false)    │
//! │    ├── "write_file" ──→ run_write()   ──→ (output, false)    │
//! │    ├── "edit_file"  ──→ run_edit()    ──→ (output, false)    │
//! │    ├── "todo"       ──→ mgr.update()  ──→ (output, true)     │
//! │    └── _            ──→ None          ──→ (None,    false)   │
//! └──────────────────────────────────────────────────────────────┘

use crate::todo_manager::TodoManager;
use crate::tool_runners::{run_bash, run_edit, run_read, run_write};
use serde_json::Value as Json;
use std::path::Path;
use std::sync::{Arc, Mutex};

// -- Tool JSON schema --
// This constant is sent to the Anthropic API so the model knows
// what tools are available and what arguments they expect.

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

// -- Tool dispatch --
// Routes a tool call by name to the appropriate handler.
// Returns `(output_string, did_use_todo)` so the agent loop
// can track whether the todo tool was invoked.

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
                Err(e) => (Some(format!("Error: {e}")), true),
            }
        }
        _ => (None, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // -- TOOLS schema validation --

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

    // -- dispatch_tools routing --

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

    #[test]
    fn test_dispatch_todo_error_returns_true() {
        let todo = Arc::new(Mutex::new(TodoManager::new()));
        let input = serde_json::json!({
            "items": [{"id": "1", "text": "", "status": "pending"}]
        });
        let (output, did_todo) = dispatch_tools("todo", &input, &PathBuf::from("."), &todo);
        assert!(did_todo, "Even errors from todo should set did_todo=true");
        assert!(output.unwrap().contains("Error:"));
    }
}
