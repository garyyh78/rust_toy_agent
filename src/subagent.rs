use crate::llm_client::AnthropicClient;
use crate::tool_runners::{run_bash, run_edit, run_read, run_write};
use serde_json::Value as Json;
use std::path::Path;

/// Subagent system that spawns child agents with fresh context.
/// The child works in its own context, sharing the filesystem,
/// then returns only a summary to the parent.
pub struct Subagent {
    client: AnthropicClient,
    workdir: String,
    model: String,
    child_tools: Json,
    parent_tools: Json,
}

impl Subagent {
    pub fn new(client: AnthropicClient, workdir: String, model: String) -> Self {
        let child_tools = serde_json::json!([
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
                    "properties": {
                        "path": {"type": "string"},
                        "limit": {"type": "integer"}
                    },
                    "required": ["path"]
                }
            },
            {
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
            },
            {
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
            }
        ]);

        let parent_tools = {
            let mut tools = child_tools.as_array().unwrap().clone();
            tools.push(serde_json::json!({
                "name": "task",
                "description": "Spawn a subagent with fresh context. It shares the filesystem but not conversation history.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "prompt": {"type": "string"},
                        "description": {"type": "string", "description": "Short description of the task"}
                    },
                    "required": ["prompt"]
                }
            }));
            serde_json::Value::Array(tools)
        };

        Self {
            client,
            workdir,
            model,
            child_tools,
            parent_tools,
        }
    }

    /// Dispatch a tool call for child agents
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
            _ => format!("Unknown tool: {}", tool_name),
        }
    }

    /// Run a subagent with fresh context (async)
    pub async fn run_subagent(&self, prompt: &str) -> String {
        let mut sub_messages = vec![serde_json::json!({
            "role": "user",
            "content": prompt
        })];

        let subagent_system = format!(
            "You are a coding subagent at {}. Complete the given task, then summarize your findings.",
            self.workdir
        );

        for _ in 0..30 {
            // safety limit
            let response = self
                .client
                .create_message(
                    &self.model,
                    Some(&subagent_system),
                    &sub_messages,
                    Some(&self.child_tools),
                    8000,
                )
                .await;

            let response = match response {
                Ok(r) => r,
                Err(e) => return format!("Error: {}", e),
            };

            sub_messages.push(serde_json::json!({
                "role": "assistant",
                "content": response["content"]
            }));

            if response["stop_reason"] != "tool_use" {
                break;
            }

            let mut results = Vec::new();
            if let Some(content) = response["content"].as_array() {
                for block in content {
                    if block["type"] == "tool_use" {
                        let tool_name = block["name"].as_str().unwrap_or("");
                        let input = &block["input"];

                        let output = self.dispatch_child_tool(tool_name, input);

                        results.push(serde_json::json!({
                            "type": "tool_result",
                            "tool_use_id": block["id"],
                            "content": output.chars().take(50000).collect::<String>()
                        }));
                    }
                }
            }

            sub_messages.push(serde_json::json!({
                "role": "user",
                "content": results
            }));
        }

        // Return only the final text summary
        if let Some(last_response) = sub_messages.last() {
            if let Some(content) = last_response["content"].as_array() {
                let text: String = content
                    .iter()
                    .filter_map(|block| {
                        if block["type"] == "text" {
                            block["text"].as_str()
                        } else {
                            None
                        }
                    })
                    .collect();
                return if text.is_empty() {
                    "(no summary)".to_string()
                } else {
                    text
                };
            }
        }

        "(no summary)".to_string()
    }

    /// Main agent loop with parent tools (async)
    pub async fn agent_loop(&self, messages: &mut Vec<Json>) {
        let system = format!(
            "You are a coding agent at {}. Use the task tool to delegate exploration or subtasks.",
            self.workdir
        );

        loop {
            let response = self
                .client
                .create_message(
                    &self.model,
                    Some(&system),
                    messages,
                    Some(&self.parent_tools),
                    8000,
                )
                .await;

            let response = match response {
                Ok(r) => r,
                Err(e) => {
                    println!("Error: {}", e);
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

            let mut results = Vec::new();
            if let Some(content) = response["content"].as_array() {
                for block in content {
                    if block["type"] == "tool_use" {
                        let tool_name = block["name"].as_str().unwrap_or("");
                        let input = &block["input"];

                        let output = if tool_name == "task" {
                            let desc = input["description"].as_str().unwrap_or("subtask");
                            let prompt = input["prompt"].as_str().unwrap_or("");
                            println!(
                                "> task ({}): {}",
                                desc,
                                &prompt[..std::cmp::min(80, prompt.len())]
                            );
                            self.run_subagent(prompt).await
                        } else {
                            self.dispatch_child_tool(tool_name, input)
                        };

                        println!("  {}", &output[..std::cmp::min(200, output.len())]);

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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_creation() {
        let client = AnthropicClient::new("test", "https://api.anthropic.com");
        let subagent = Subagent::new(client, "/tmp".to_string(), "test-model".to_string());

        assert_eq!(subagent.workdir, "/tmp");
        assert_eq!(subagent.model, "test-model");

        // Verify child tools
        let child_tools = subagent.child_tools.as_array().unwrap();
        assert_eq!(child_tools.len(), 4);

        // Verify parent tools (child + task)
        let parent_tools = subagent.parent_tools.as_array().unwrap();
        assert_eq!(parent_tools.len(), 5);
        assert_eq!(parent_tools[4]["name"], "task");
    }

    #[test]
    fn test_dispatch_child_tool() {
        let client = AnthropicClient::new("test", "https://api.anthropic.com");
        let subagent = Subagent::new(client, "/tmp".to_string(), "test-model".to_string());

        // Test bash tool
        let input = serde_json::json!({"command": "echo hello"});
        let result = subagent.dispatch_child_tool("bash", &input);
        assert!(result.contains("hello"));

        // Test unknown tool
        let result = subagent.dispatch_child_tool("unknown", &serde_json::json!({}));
        assert!(result.contains("Unknown tool"));
    }

    #[test]
    fn test_dispatch_read_file() {
        let client = AnthropicClient::new("test", "https://api.anthropic.com");
        let subagent = Subagent::new(client, "/tmp".to_string(), "test-model".to_string());

        let input = serde_json::json!({"path": "Cargo.toml", "limit": 5});
        let result = subagent.dispatch_child_tool("read_file", &input);
        // Should either read the file or show an error (not panic)
        assert!(!result.is_empty());
    }
}
