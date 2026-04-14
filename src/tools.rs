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
use crate::tool_runners::{run_bash, run_edit, run_read, run_write, WorkdirRoot};
use serde_json::Value as Json;
use std::sync::{Arc, Mutex};

// -- Tool JSON schema builders --
// Each function returns a serde_json::Value for a specific tool.
// This eliminates duplication across the codebase.

/// Core file operation tools
pub fn tool_bash() -> Json {
    serde_json::json!({
        "name": "bash",
        "description": "Run a shell command.",
        "input_schema": {
            "type": "object",
            "properties": {"command": {"type": "string"}},
            "required": ["command"]
        }
    })
}

pub fn tool_read_file() -> Json {
    serde_json::json!({
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
    })
}

pub fn tool_write_file() -> Json {
    serde_json::json!({
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
    })
}

pub fn tool_edit_file() -> Json {
    serde_json::json!({
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
    })
}

pub fn tool_todo() -> Json {
    serde_json::json!({
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
    })
}

/// Extended tools for full agent
pub fn tool_todo_write() -> Json {
    serde_json::json!({
        "name": "TodoWrite",
        "description": "Update task tracking list.",
        "input_schema": {
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {"type": "string"},
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"]
                            }
                        },
                        "required": ["content", "status"]
                    }
                }
            },
            "required": ["items"]
        }
    })
}

pub fn tool_task() -> Json {
    serde_json::json!({
        "name": "task",
        "description": "Spawn a subagent for isolated exploration or work.",
        "input_schema": {
            "type": "object",
            "properties": {
                "prompt": {"type": "string"},
                "description": {"type": "string"}
            },
            "required": ["prompt"]
        }
    })
}

pub fn tool_load_skill() -> Json {
    serde_json::json!({
        "name": "load_skill",
        "description": "Load specialized knowledge by name.",
        "input_schema": {
            "type": "object",
            "properties": {"name": {"type": "string"}},
            "required": ["name"]
        }
    })
}

pub fn tool_compact() -> Json {
    serde_json::json!({
        "name": "compact",
        "description": "Manually compact conversation context.",
        "input_schema": {
            "type": "object",
            "properties": {
                "focus": {"type": "string", "description": "What to preserve in the summary"}
            }
        }
    })
}

pub fn tool_background_run() -> Json {
    serde_json::json!({
        "name": "background_run",
        "description": "Run command in background thread.",
        "input_schema": {
            "type": "object",
            "properties": {
                "command": {"type": "string"},
                "timeout": {"type": "integer"}
            },
            "required": ["command"]
        }
    })
}

pub fn tool_check_background() -> Json {
    serde_json::json!({
        "name": "check_background",
        "description": "Check background task status.",
        "input_schema": {
            "type": "object",
            "properties": {"task_id": {"type": "string"}}
        }
    })
}

/// Task system tools
pub fn tool_task_create() -> Json {
    serde_json::json!({
        "name": "task_create",
        "description": "Create a persistent file task.",
        "input_schema": {
            "type": "object",
            "properties": {
                "subject": {"type": "string"},
                "description": {"type": "string"}
            },
            "required": ["subject"]
        }
    })
}

pub fn tool_task_get() -> Json {
    serde_json::json!({
        "name": "task_get",
        "description": "Get task details by ID.",
        "input_schema": {
            "type": "object",
            "properties": {"task_id": {"type": "integer"}},
            "required": ["task_id"]
        }
    })
}

pub fn tool_task_update() -> Json {
    serde_json::json!({
        "name": "task_update",
        "description": "Update task status or dependencies.",
        "input_schema": {
            "type": "object",
            "properties": {
                "task_id": {"type": "integer"},
                "status": {
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed"]
                },
                "add_blocked_by": {"type": "array", "items": {"type": "integer"}},
                "add_blocks": {"type": "array", "items": {"type": "integer"}}
            },
            "required": ["task_id"]
        }
    })
}

pub fn tool_task_list() -> Json {
    serde_json::json!({
        "name": "task_list",
        "description": "List all tasks.",
        "input_schema": {"type": "object", "properties": {}}
    })
}

/// Team tools
pub fn tool_spawn_teammate() -> Json {
    serde_json::json!({
        "name": "spawn_teammate",
        "description": "Spawn a persistent autonomous teammate.",
        "input_schema": {
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "role": {"type": "string"},
                "prompt": {"type": "string"}
            },
            "required": ["name", "role", "prompt"]
        }
    })
}

pub fn tool_list_teammates() -> Json {
    serde_json::json!({
        "name": "list_teammates",
        "description": "List all teammates.",
        "input_schema": {"type": "object", "properties": {}}
    })
}

pub fn tool_send_message() -> Json {
    serde_json::json!({
        "name": "send_message",
        "description": "Send a message to a teammate.",
        "input_schema": {
            "type": "object",
            "properties": {
                "to": {"type": "string"},
                "content": {"type": "string"},
                "msg_type": {
                    "type": "string",
                    "enum": ["message", "broadcast", "shutdown_request", "shutdown_response", "plan_approval_response"]
                }
            },
            "required": ["to", "content"]
        }
    })
}

pub fn tool_read_inbox() -> Json {
    serde_json::json!({
        "name": "read_inbox",
        "description": "Read and drain the lead's inbox.",
        "input_schema": {"type": "object", "properties": {}}
    })
}

pub fn tool_broadcast() -> Json {
    serde_json::json!({
        "name": "broadcast",
        "description": "Send message to all teammates.",
        "input_schema": {
            "type": "object",
            "properties": {"content": {"type": "string"}},
            "required": ["content"]
        }
    })
}

pub fn tool_shutdown_request() -> Json {
    serde_json::json!({
        "name": "shutdown_request",
        "description": "Request a teammate to shut down.",
        "input_schema": {
            "type": "object",
            "properties": {"teammate": {"type": "string"}},
            "required": ["teammate"]
        }
    })
}

pub fn tool_plan_approval() -> Json {
    serde_json::json!({
        "name": "plan_approval",
        "description": "Approve or reject a teammate's plan.",
        "input_schema": {
            "type": "object",
            "properties": {
                "request_id": {"type": "string"},
                "approve": {"type": "boolean"},
                "feedback": {"type": "string"}
            },
            "required": ["request_id", "approve"]
        }
    })
}

pub fn tool_idle() -> Json {
    serde_json::json!({
        "name": "idle",
        "description": "Enter idle state.",
        "input_schema": {"type": "object", "properties": {}}
    })
}

pub fn tool_claim_task() -> Json {
    serde_json::json!({
        "name": "claim_task",
        "description": "Claim a task from the board.",
        "input_schema": {
            "type": "object",
            "properties": {"task_id": {"type": "integer"}},
            "required": ["task_id"]
        }
    })
}

pub fn tool_worktree_create() -> Json {
    serde_json::json!({
        "name": "worktree_create",
        "description": "Create a git worktree for isolated branch work.",
        "input_schema": {
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "task_id": {"type": "integer"},
                "base_ref": {"type": "string"}
            },
            "required": ["name"]
        }
    })
}

pub fn tool_worktree_list() -> Json {
    serde_json::json!({
        "name": "worktree_list",
        "description": "List all worktrees.",
        "input_schema": {"type": "object", "properties": {}}
    })
}

pub fn tool_worktree_remove() -> Json {
    serde_json::json!({
        "name": "worktree_remove",
        "description": "Remove a worktree.",
        "input_schema": {
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "force": {"type": "boolean"},
                "complete_task": {"type": "boolean"}
            },
            "required": ["name"]
        }
    })
}

// -- Convenience collections --

/// Core file tools (bash, read_file, write_file, edit_file)
pub fn core_file_tools() -> Vec<Json> {
    vec![
        tool_bash(),
        tool_read_file(),
        tool_write_file(),
        tool_edit_file(),
    ]
}

/// Core tools + todo (for basic agents)
pub fn core_tools_with_todo() -> Vec<Json> {
    let mut tools = core_file_tools();
    tools.push(tool_todo());
    tools
}

/// Child agent tools (core file tools, no todo)
pub fn child_agent_tools() -> Vec<Json> {
    core_file_tools()
}

/// Parent agent tools (child tools + task delegation)
pub fn parent_agent_tools() -> Vec<Json> {
    let mut tools = child_agent_tools();
    tools.push(tool_task());
    tools
}

/// Full agent tools (everything)
pub fn full_agent_tools() -> Vec<Json> {
    vec![
        tool_bash(),
        tool_read_file(),
        tool_write_file(),
        tool_edit_file(),
        tool_todo_write(),
        tool_task(),
        tool_load_skill(),
        tool_compact(),
        tool_background_run(),
        tool_check_background(),
        tool_task_create(),
        tool_task_get(),
        tool_task_update(),
        tool_task_list(),
        tool_spawn_teammate(),
        tool_list_teammates(),
        tool_send_message(),
        tool_read_inbox(),
        tool_broadcast(),
        tool_shutdown_request(),
        tool_plan_approval(),
        tool_idle(),
        tool_claim_task(),
        tool_worktree_create(),
        tool_worktree_list(),
        tool_worktree_remove(),
    ]
}

/// Teammate agent tools (file tools + messaging + idle + claim_task)
pub fn teammate_tools() -> Vec<Json> {
    vec![
        tool_bash(),
        tool_read_file(),
        tool_write_file(),
        tool_edit_file(),
        tool_send_message(),
        tool_idle(),
        tool_claim_task(),
    ]
}

/// Skill loading agent tools (file tools + load_skill)
pub fn skill_agent_tools() -> Vec<Json> {
    let mut tools = core_file_tools();
    tools.push(tool_load_skill());
    tools
}

/// Context compactor tools (file tools + compact)
pub fn compactor_tools() -> Vec<Json> {
    let mut tools = core_file_tools();
    tools.push(tool_compact());
    tools
}

// -- Legacy TOOLS constant (deprecated, use core_tools_with_todo() or child_agent_tools()) --
// This constant is kept for backward compatibility during the transition.
// New code should use the builder functions above.

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
    workdir: &WorkdirRoot,
    todo: &Arc<Mutex<TodoManager>>,
) -> (Option<String>, bool) {
    match tool_name {
        "bash" => (
            Some(run_bash(
                input["command"].as_str().unwrap_or(""),
                workdir.as_path(),
            )),
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
            let mut mgr = match todo.lock() {
                Ok(m) => m,
                Err(e) => return (Some(format!("Error: lock poisoned: {}", e)), true),
            };
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
    use crate::tool_runners::WorkdirRoot;
    use std::path::PathBuf;

    // -- Tool builder validation --

    #[test]
    fn test_tool_bash_schema() {
        let tool = tool_bash();
        assert_eq!(tool["name"], "bash");
        assert_eq!(tool["description"], "Run a shell command.");
        let schema = &tool["input_schema"];
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["command"].is_object());
    }

    #[test]
    fn test_tool_read_file_schema() {
        let tool = tool_read_file();
        assert_eq!(tool["name"], "read_file");
        let schema = &tool["input_schema"];
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["limit"].is_object());
    }

    #[test]
    fn test_tool_write_file_schema() {
        let tool = tool_write_file();
        assert_eq!(tool["name"], "write_file");
        let schema = &tool["input_schema"];
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required.contains(&"path"));
        assert!(required.contains(&"content"));
    }

    #[test]
    fn test_tool_edit_file_schema() {
        let tool = tool_edit_file();
        assert_eq!(tool["name"], "edit_file");
        let schema = &tool["input_schema"];
        assert!(schema["properties"]["old_text"].is_object());
        assert!(schema["properties"]["new_text"].is_object());
    }

    #[test]
    fn test_tool_todo_schema() {
        let tool = tool_todo();
        assert_eq!(tool["name"], "todo");
        let schema = &tool["input_schema"];
        let items = &schema["properties"]["items"];
        assert_eq!(items["type"], "array");
        let item_props = &items["items"]["properties"];
        assert!(item_props["id"].is_object());
        assert!(item_props["text"].is_object());
        assert!(item_props["status"].is_object());
    }

    #[test]
    fn test_full_agent_tools_count() {
        let tools = full_agent_tools();
        // Should have all 26 tools (23 + 3 worktree)
        assert_eq!(tools.len(), 26);

        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"TodoWrite"));
        assert!(names.contains(&"task"));
        assert!(names.contains(&"load_skill"));
        assert!(names.contains(&"compact"));
        assert!(names.contains(&"background_run"));
        assert!(names.contains(&"check_background"));
        assert!(names.contains(&"task_create"));
        assert!(names.contains(&"task_get"));
        assert!(names.contains(&"task_update"));
        assert!(names.contains(&"task_list"));
        assert!(names.contains(&"spawn_teammate"));
        assert!(names.contains(&"list_teammates"));
        assert!(names.contains(&"send_message"));
        assert!(names.contains(&"read_inbox"));
        assert!(names.contains(&"broadcast"));
        assert!(names.contains(&"shutdown_request"));
        assert!(names.contains(&"plan_approval"));
        assert!(names.contains(&"idle"));
        assert!(names.contains(&"claim_task"));
    }

    #[test]
    fn test_core_file_tools_count() {
        let tools = core_file_tools();
        assert_eq!(tools.len(), 4);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"edit_file"));
    }

    #[test]
    fn test_teammate_tools_count() {
        let tools = teammate_tools();
        assert_eq!(tools.len(), 7);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"send_message"));
        assert!(names.contains(&"idle"));
        assert!(names.contains(&"claim_task"));
    }

    #[test]
    fn test_skill_agent_tools_count() {
        let tools = skill_agent_tools();
        assert_eq!(tools.len(), 5);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"load_skill"));
    }

    #[test]
    fn test_compactor_tools_count() {
        let tools = compactor_tools();
        assert_eq!(tools.len(), 5);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"compact"));
    }

    // -- Legacy TOOLS schema validation --

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
        let wd = WorkdirRoot::new(&PathBuf::from(".")).unwrap();
        let (output, did_todo) = dispatch_tools("todo", &input, &wd, &todo);
        assert!(did_todo);
        assert!(output.unwrap().contains("[ ] #1: Test task"));
    }

    #[test]
    fn test_dispatch_bash_not_todo() {
        let todo = Arc::new(Mutex::new(TodoManager::new()));
        let input = serde_json::json!({"command": "echo hello"});
        let wd = WorkdirRoot::new(&PathBuf::from(".")).unwrap();
        let (output, did_todo) = dispatch_tools("bash", &input, &wd, &todo);
        assert!(!did_todo);
        assert!(output.unwrap().contains("hello"));
    }

    #[test]
    fn test_dispatch_unknown_tool() {
        let todo = Arc::new(Mutex::new(TodoManager::new()));
        let input = serde_json::json!({"foo": "bar"});
        let wd = WorkdirRoot::new(&PathBuf::from(".")).unwrap();
        let (output, did_todo) = dispatch_tools("unknown_tool", &input, &wd, &todo);
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
        let wd = WorkdirRoot::new(&PathBuf::from(".")).unwrap();
        let (output, did_todo) = dispatch_tools("todo", &input, &wd, &todo);
        assert!(did_todo, "Even errors from todo should set did_todo=true");
        assert!(output.unwrap().contains("Error:"));
    }
}
