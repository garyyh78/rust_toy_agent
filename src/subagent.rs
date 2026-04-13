use crate::config::SUBAGENT_MAX_TOKENS;
use crate::llm_client::AnthropicClient;
use crate::tool_runners::{run_bash, run_edit, run_read, run_write};
use crate::tools::{child_agent_tools, parent_agent_tools};
use serde_json::Value as Json;
use std::path::Path;

/// Maximum iterations for a subagent loop (safety limit).
const MAX_SUBAGENT_TURNS: u32 = 30;
/// Truncate tool output to this many characters before returning to the LLM.
const MAX_TOOL_OUTPUT: usize = 50_000;

/// Subagent system that spawns child agents with fresh context.
/// The child works in its own context, sharing the filesystem,
/// then returns only a summary to the parent.
#[allow(dead_code)]
pub struct Subagent {
    client: AnthropicClient,
    workdir: String,
    model: String,
    child_tools: Json,
    parent_tools: Json,
}

impl Subagent {
    pub fn new(client: AnthropicClient, workdir: String, model: String) -> Self {
        let child_tools = Json::Array(child_agent_tools());

        let parent_tools_array = parent_agent_tools();
        let parent_tools = Json::Array(
            parent_tools_array
                .iter()
                .filter(|t| {
                    t.get("name")
                        .and_then(|v| v.as_str())
                        .map(|s| s != "todo")
                        .unwrap_or(true)
                })
                .cloned()
                .collect(),
        );

        Self {
            client,
            workdir,
            model,
            child_tools,
            parent_tools,
        }
    }

    /// Dispatch a tool call for child agents.
    fn dispatch_child_tool(&self, tool_name: &str, input: &Json) -> String {
        let workdir = Path::new(&self.workdir);
        match tool_name {
            "bash" => run_bash(input["command"].as_str().unwrap_or(""), workdir),
            "read_file" => run_read(
                input["path"].as_str().unwrap_or(""),
                input["limit"].as_u64().map(|n| n as usize),
                workdir,
            ),
            "write_file" => run_write(
                input["path"].as_str().unwrap_or(""),
                input["content"].as_str().unwrap_or(""),
                workdir,
            ),
            "edit_file" => run_edit(
                input["path"].as_str().unwrap_or(""),
                input["old_text"].as_str().unwrap_or(""),
                input["new_text"].as_str().unwrap_or(""),
                workdir,
            ),
            _ => format!("Unknown tool: {tool_name}"),
        }
    }

    /// Execute all tool_use blocks in a response and return the results.
    fn execute_tool_calls(&self, response: &Json) -> Vec<Json> {
        let mut results = Vec::new();
        if let Some(content) = response["content"].as_array() {
            for block in content {
                if block["type"] != "tool_use" {
                    continue;
                }
                let tool_name = block["name"].as_str().unwrap_or("");
                let output = self.dispatch_child_tool(tool_name, &block["input"]);
                results.push(serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": block["id"],
                    "content": output.chars().take(MAX_TOOL_OUTPUT).collect::<String>()
                }));
            }
        }
        results
    }

    /// Extract the final text summary from the last assistant message.
    fn extract_summary(messages: &[Json]) -> String {
        let Some(last) = messages.last() else {
            return "(no summary)".to_string();
        };
        let Some(content) = last["content"].as_array() else {
            return "(no summary)".to_string();
        };
        let text: String = content
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect();
        if text.is_empty() {
            "(no summary)".to_string()
        } else {
            text
        }
    }

    /// Run a subagent with fresh context (async).
    pub async fn run_subagent(&self, prompt: &str) -> String {
        let mut messages = vec![serde_json::json!({
            "role": "user",
            "content": prompt
        })];

        let system = format!(
            "You are a coding subagent at {}. Complete the given task, then summarize your findings.",
            self.workdir
        );

        for _ in 0..MAX_SUBAGENT_TURNS {
            let response = match self
                .client
                .create_message(
                    &self.model,
                    Some(&system),
                    &messages,
                    Some(&self.child_tools),
                    SUBAGENT_MAX_TOKENS,
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return format!("Error: {e}"),
            };

            messages.push(serde_json::json!({
                "role": "assistant",
                "content": response["content"]
            }));

            if response["stop_reason"] != "tool_use" {
                break;
            }

            let results = self.execute_tool_calls(&response);
            messages.push(serde_json::json!({
                "role": "user",
                "content": results
            }));
        }

        Self::extract_summary(&messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_client() -> AnthropicClient {
        AnthropicClient::new("test", "https://api.anthropic.com")
    }

    fn test_subagent(workdir: &str) -> Subagent {
        Subagent::new(test_client(), workdir.to_string(), "test-model".to_string())
    }

    fn tmp_workdir() -> (TempDir, String) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().to_str().unwrap().to_string();
        (tmp, path)
    }

    // -- construction --

    #[test]
    fn test_subagent_creation() {
        let (_tmp, workdir) = tmp_workdir();
        let sub = test_subagent(&workdir);

        assert_eq!(sub.workdir, workdir);
        assert_eq!(sub.model, "test-model");

        let child_tools = sub.child_tools.as_array().unwrap();
        assert_eq!(child_tools.len(), 4);

        let parent_tools = sub.parent_tools.as_array().unwrap();
        assert_eq!(parent_tools.len(), 5);
        assert_eq!(parent_tools[4]["name"], "task");
    }

    #[test]
    fn test_subagent_new_with_different_workdir() {
        let (_tmp, workdir) = tmp_workdir();
        let sub = test_subagent(&workdir);
        assert_eq!(sub.workdir, workdir);
    }

    // -- tool filtering --

    #[test]
    fn test_child_tools_filter_handles_malformed_name() {
        let (_tmp, workdir) = tmp_workdir();
        let _sub = test_subagent(&workdir);

        let tools_with_malformed = vec![
            serde_json::json!({"name": "bash", "description": "Run command"}),
            serde_json::json!({"name": 123, "description": "name is a number"}),
            serde_json::json!({"description": "no name field"}),
            serde_json::json!({"name": "read_file", "description": "Read file"}),
            serde_json::json!({"name": "todo", "description": "should be filtered"}),
        ];
        let filtered: Vec<Json> = tools_with_malformed
            .iter()
            .filter(|t| {
                t.get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s != "todo")
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        assert_eq!(filtered.len(), 4);
        assert!(filtered
            .iter()
            .all(|t| t.get("name").and_then(|v| v.as_str()) != Some("todo")));
    }

    #[test]
    fn test_child_tools_excludes_todo() {
        let (_tmp, workdir) = tmp_workdir();
        let sub = test_subagent(&workdir);

        let names: Vec<&str> = sub
            .child_tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(
            !names.contains(&"todo"),
            "child tools must not include todo"
        );
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"edit_file"));
    }

    #[test]
    fn test_parent_tools_has_task_with_correct_schema() {
        let (_tmp, workdir) = tmp_workdir();
        let sub = test_subagent(&workdir);

        let parent_tools = sub.parent_tools.as_array().unwrap();
        let task_tool = parent_tools.iter().find(|t| t["name"] == "task").unwrap();

        let schema = &task_tool["input_schema"];
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["prompt"].is_object());
        assert!(schema["properties"]["description"].is_object());

        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required.contains(&"prompt"));
    }

    #[test]
    fn test_parent_tools_includes_all_child_tools() {
        let sub = test_subagent("/tmp");

        let child_names: Vec<&str> = sub
            .child_tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();

        let parent_names: Vec<&str> = sub
            .parent_tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();

        for name in &child_names {
            assert!(
                parent_names.contains(name),
                "parent missing child tool: {name}"
            );
        }
        assert!(parent_names.contains(&"task"));
        assert!(!parent_names.contains(&"todo"));
    }

    // -- dispatch_child_tool: happy paths --

    #[test]
    fn test_dispatch_bash() {
        let sub = test_subagent("/tmp");
        let result = sub.dispatch_child_tool("bash", &serde_json::json!({"command": "echo hello"}));
        assert!(result.contains("hello"));
    }

    #[test]
    fn test_dispatch_read_file() {
        let (_tmp, workdir) = tmp_workdir();
        let sub = test_subagent(&workdir);
        let result = sub.dispatch_child_tool(
            "read_file",
            &serde_json::json!({"path": "Cargo.toml", "limit": 5}),
        );
        assert!(!result.is_empty());
    }

    #[test]
    fn test_dispatch_write_file() {
        let (tmp_dir, workdir) = tmp_workdir();
        let sub = test_subagent(&workdir);

        let filename = "write_test.txt";
        let tmp = tmp_dir.path().join(filename);
        let result = sub.dispatch_child_tool(
            "write_file",
            &serde_json::json!({"path": filename, "content": "subagent wrote this"}),
        );
        assert!(result.contains("Wrote"));

        assert_eq!(
            std::fs::read_to_string(&tmp).unwrap(),
            "subagent wrote this"
        );
        drop(tmp_dir);
    }

    #[test]
    fn test_dispatch_edit_file() {
        let (tmp_dir, workdir) = tmp_workdir();
        let sub = test_subagent(&workdir);

        let filename = "edit_test.txt";
        let tmp = tmp_dir.path().join(filename);
        let _ = std::fs::write(&tmp, "replace ME please");

        let result = sub.dispatch_child_tool(
            "edit_file",
            &serde_json::json!({
                "path": filename,
                "old_text": "ME",
                "new_text": "YOU"
            }),
        );
        assert!(result.contains("Edited"));

        assert_eq!(std::fs::read_to_string(&tmp).unwrap(), "replace YOU please");
        drop(tmp_dir);
    }

    #[test]
    fn test_dispatch_unknown_tool() {
        let sub = test_subagent("/tmp");
        let result = sub.dispatch_child_tool("unknown", &serde_json::json!({}));
        assert!(result.contains("Unknown tool"));
    }

    // -- dispatch_child_tool: edge cases --

    #[test]
    fn test_dispatch_bash_missing_command() {
        let sub = test_subagent("/tmp");
        let result = sub.dispatch_child_tool("bash", &serde_json::json!({}));
        assert!(!result.is_empty());
    }

    #[test]
    fn test_dispatch_read_file_missing_path() {
        let sub = test_subagent("/tmp");
        let result = sub.dispatch_child_tool("read_file", &serde_json::json!({}));
        assert!(result.contains("Error"));
    }

    #[test]
    fn test_dispatch_write_file_missing_fields() {
        let sub = test_subagent("/tmp");
        let result =
            sub.dispatch_child_tool("write_file", &serde_json::json!({"path": "/tmp/test.txt"}));
        assert!(!result.is_empty());
    }

    #[test]
    fn test_dispatch_edit_file_text_not_found() {
        let (tmp_dir, workdir) = tmp_workdir();
        let sub = test_subagent(&workdir);

        let filename = "edit_nf_test.txt";
        let tmp = tmp_dir.path().join(filename);
        let _ = std::fs::write(&tmp, "hello world");

        let result = sub.dispatch_child_tool(
            "edit_file",
            &serde_json::json!({
                "path": filename,
                "old_text": "missing",
                "new_text": "replaced"
            }),
        );
        assert!(result.contains("Text not found"));
        drop(tmp_dir);
    }

    #[test]
    fn test_dispatch_bash_dangerous_blocked() {
        let sub = test_subagent("/tmp");
        let result =
            sub.dispatch_child_tool("bash", &serde_json::json!({"command": "sudo rm -rf /"}));
        assert!(result.contains("Dangerous command blocked"));
    }

    // -- extract_summary --

    #[test]
    fn test_extract_summary_from_text_blocks() {
        let messages = vec![serde_json::json!({
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Found 3 files."},
                {"type": "text", "text": " All look correct."}
            ]
        })];
        assert_eq!(
            Subagent::extract_summary(&messages),
            "Found 3 files. All look correct."
        );
    }

    #[test]
    fn test_extract_summary_empty_messages() {
        assert_eq!(Subagent::extract_summary(&[]), "(no summary)");
    }

    #[test]
    fn test_extract_summary_no_text_blocks() {
        let messages = vec![serde_json::json!({
            "role": "assistant",
            "content": [{"type": "tool_use", "id": "x", "name": "bash", "input": {}}]
        })];
        assert_eq!(Subagent::extract_summary(&messages), "(no summary)");
    }
}
