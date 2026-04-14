use crate::config::{COMPACT_THRESHOLD as COMPACT_TOKEN_THRESHOLD, SUMMARIZE_MAX_TOKENS};
use crate::llm_client::AnthropicClient;
use crate::tool_runners::{run_bash, run_edit, run_read, run_write, WorkdirRoot};
use crate::tools::compactor_tools;
use serde_json::Value as Json;
use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Truncate tool result previews to this length.
const TOOL_RESULT_PREVIEW_LEN: usize = 100;

/// Maximum conversation size before forced truncation.
const MAX_CONVERSATION_SIZE: usize = 80_000;

/// Three-layer compression pipeline for context management.
/// Layer 1: micro_compact - replace old tool results with placeholders
/// Layer 2: auto_compact - save transcript, summarize, replace messages
/// Layer 3: manual_compact - triggered by compact tool
#[allow(dead_code)]
pub struct ContextCompactor {
    client: AnthropicClient,
    workdir: String,
    model: String,
    threshold: usize,
    keep_recent: usize,
    transcript_dir: String,
    tools: Json,
}

impl ContextCompactor {
    pub fn new(client: AnthropicClient, workdir: String, model: String) -> Self {
        let tools = Json::Array(compactor_tools());

        Self {
            client,
            workdir: workdir.clone(),
            model,
            threshold: COMPACT_TOKEN_THRESHOLD,
            keep_recent: 3,
            transcript_dir: format!("{workdir}/.transcripts"),
            tools,
        }
    }

    /// Rough token count: ~4 chars per token
    pub fn estimate_tokens(messages: &[Json]) -> usize {
        let total_chars: usize = messages
            .iter()
            .map(|m| serde_json::to_string(m).unwrap_or_default().len())
            .sum();
        total_chars / 4
    }

    /// Layer 1: micro_compact - replace old tool results with placeholders
    pub fn micro_compact(&self, messages: &mut [Json]) {
        // Collect tool results with their indices
        let mut tool_results: Vec<(usize, usize, Json)> = Vec::new();

        for (msg_idx, msg) in messages.iter().enumerate() {
            if msg["role"] == "user" {
                if let Some(content) = msg["content"].as_array() {
                    for (part_idx, part) in content.iter().enumerate() {
                        if part["type"] == "tool_result" {
                            tool_results.push((msg_idx, part_idx, part.clone()));
                        }
                    }
                }
            }
        }

        if tool_results.len() <= self.keep_recent {
            return;
        }

        // Build tool name map from assistant messages
        let mut tool_name_map: HashMap<String, String> = HashMap::new();
        for msg in messages.iter() {
            if msg["role"] == "assistant" {
                if let Some(content) = msg["content"].as_array() {
                    for block in content {
                        if block["type"] == "tool_use" {
                            if let (Some(id), Some(name)) =
                                (block["id"].as_str(), block["name"].as_str())
                            {
                                tool_name_map.insert(id.to_string(), name.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Clear old results (keep last keep_recent)
        let to_clear = &tool_results[..tool_results.len() - self.keep_recent];
        for (msg_idx, part_idx, _) in to_clear {
            if let Some(msg) = messages.get_mut(*msg_idx) {
                if let Some(content) = msg["content"].as_array_mut() {
                    if let Some(result) = content.get_mut(*part_idx) {
                        if let Some(content_str) = result["content"].as_str() {
                            if content_str.len() > TOOL_RESULT_PREVIEW_LEN {
                                let tool_id = result["tool_use_id"].as_str().unwrap_or("");
                                let tool_name = tool_name_map
                                    .get(tool_id)
                                    .map(|s| s.as_str())
                                    .unwrap_or("unknown");
                                result["content"] =
                                    Json::String(format!("[Previous: used {tool_name}]"));
                            }
                        }
                    }
                }
            }
        }
    }

    /// Layer 2: auto_compact - save transcript, summarize, replace messages (async)
    pub async fn auto_compact(&self, messages: &[Json]) -> Vec<Json> {
        let transcript_path = format!(
            "{}/transcript_{}.jsonl",
            self.transcript_dir,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );

        if let Err(e) = std::fs::create_dir_all(&self.transcript_dir) {
            tracing::error!(error = %e, "failed to create transcript dir");
        } else if let Ok(mut file) = std::fs::File::create(&transcript_path) {
            for msg in messages {
                if let Ok(json) = serde_json::to_string(msg) {
                    use std::io::Write;
                    if let Err(e) = writeln!(file, "{json}") {
                        tracing::error!(error = %e, "failed to write transcript");
                    }
                }
            }
        }

        tracing::info!(path = %transcript_path, "transcript saved");

        // Prepare conversation for summarization
        let conversation_text = serde_json::to_string(messages).unwrap_or_default();
        let truncated = if conversation_text.len() > MAX_CONVERSATION_SIZE {
            &conversation_text[..MAX_CONVERSATION_SIZE]
        } else {
            &conversation_text
        };

        // Call LLM to summarize
        let summary_messages = vec![serde_json::json!({
            "role": "user",
            "content": format!(
                "Summarize this conversation for continuity. Include: \
                 1) What was accomplished, 2) Current state, 3) Key decisions made. \
                 Be concise but preserve critical details.\n\n{}",
                truncated
            )
        })];

        let response = self
            .client
            .create_message(
                &self.model,
                None, // no system prompt for summarization
                &summary_messages,
                None, // no tools for summarization
                SUMMARIZE_MAX_TOKENS,
            )
            .await;

        let summary = match response {
            Ok(r) => r["content"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|block| block["text"].as_str())
                .unwrap_or("(no summary)")
                .to_string(),
            Err(_) => "(summarization failed)".to_string(),
        };

        // Replace all messages with compressed summary
        vec![
            serde_json::json!({
                "role": "user",
                "content": format!("[Conversation compressed. Transcript: {}]\n\n{}", transcript_path, summary)
            }),
            serde_json::json!({
                "role": "assistant",
                "content": "Understood. I have the context from the summary. Continuing."
            }),
        ]
    }

    #[allow(dead_code)]
    /// Dispatch a tool call
    fn dispatch_tool(&self, tool_name: &str, input: &Json) -> String {
        let workdir = Path::new(&self.workdir);
        let wd = match WorkdirRoot::new(workdir) {
            Ok(w) => w,
            Err(e) => return format!("Error: workdir: {e}"),
        };
        match tool_name {
            "bash" => run_bash(input["command"].as_str().unwrap_or(""), workdir),
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
            "compact" => "Manual compression requested.".to_string(),
            _ => format!("Unknown tool: {tool_name}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        let messages = vec![serde_json::json!({"role": "user", "content": "test message"})];
        let tokens = ContextCompactor::estimate_tokens(&messages);
        assert!(tokens > 0);
    }

    #[test]
    fn test_micro_compact() {
        let client = AnthropicClient::new("test", "https://api.anthropic.com");
        let compactor = ContextCompactor::new(client, "/tmp".to_string(), "test".to_string());

        // Create messages with 4 tool results (more than keep_recent=3)
        let mut messages = vec![
            serde_json::json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "tool_1",
                    "name": "bash",
                    "input": {"command": "echo test1"}
                }]
            }),
            serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "tool_1",
                    "content": "long output 1 that should be compressed because it exceeds 100 characters and this is definitely more than that threshold"
                }]
            }),
            serde_json::json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "tool_2",
                    "name": "bash",
                    "input": {"command": "echo test2"}
                }]
            }),
            serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "tool_2",
                    "content": "long output 2 that should be compressed because it exceeds 100 characters and this is definitely more than that threshold"
                }]
            }),
            serde_json::json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "tool_3",
                    "name": "bash",
                    "input": {"command": "echo test3"}
                }]
            }),
            serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "tool_3",
                    "content": "long output 3 that should be compressed because it exceeds 100 characters and this is definitely more than that threshold"
                }]
            }),
            serde_json::json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "tool_4",
                    "name": "bash",
                    "input": {"command": "echo test4"}
                }]
            }),
            serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "tool_4",
                    "content": "long output 4 that should NOT be compressed because it's recent"
                }]
            }),
        ];

        compactor.micro_compact(&mut messages);

        // Check that old tool results (tool_1, tool_2, tool_3) were compressed
        // tool_4 should NOT be compressed (it's in the last 3)
        if let Some(user_msg) = messages.get(1) {
            if let Some(content) = user_msg["content"].as_array() {
                if let Some(result) = content.first() {
                    let content_str = result["content"].as_str().unwrap_or("");
                    assert!(
                        content_str.contains("[Previous: used bash]"),
                        "tool_1 should be compressed"
                    );
                }
            }
        }

        // tool_4 should NOT be compressed
        if let Some(user_msg) = messages.get(7) {
            if let Some(content) = user_msg["content"].as_array() {
                if let Some(result) = content.first() {
                    let content_str = result["content"].as_str().unwrap_or("");
                    assert!(
                        !content_str.contains("[Previous:"),
                        "tool_4 should NOT be compressed"
                    );
                }
            }
        }
    }

    #[test]
    fn test_micro_compact_no_compress_recent() {
        let client = AnthropicClient::new("test", "https://api.anthropic.com");
        let compactor = ContextCompactor::new(client, "/tmp".to_string(), "test".to_string());

        let mut messages = vec![
            serde_json::json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "tool_1",
                    "name": "bash",
                    "input": {"command": "echo test"}
                }]
            }),
            serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "tool_1",
                    "content": "recent output that should not be compressed"
                }]
            }),
        ];

        let original = messages.clone();
        compactor.micro_compact(&mut messages);

        // Messages should be unchanged (only 1 tool result, less than keep_recent)
        assert_eq!(messages, original);
    }

    #[test]
    fn test_dispatch_tool() {
        let client = AnthropicClient::new("test", "https://api.anthropic.com");
        let compactor = ContextCompactor::new(client, "/tmp".to_string(), "test".to_string());

        // Test bash tool
        let input = serde_json::json!({"command": "echo hello"});
        let result = compactor.dispatch_tool("bash", &input);
        assert!(result.contains("hello"));

        // Test compact tool
        let result = compactor.dispatch_tool("compact", &serde_json::json!({}));
        assert!(result.contains("Manual compression"));

        // Test unknown tool
        let result = compactor.dispatch_tool("unknown", &serde_json::json!({}));
        assert!(result.contains("Unknown tool"));
    }

    #[test]
    fn test_context_compactor_creation() {
        let client = AnthropicClient::new("test", "https://api.anthropic.com");
        let compactor = ContextCompactor::new(client, "/tmp".to_string(), "test-model".to_string());

        assert_eq!(compactor.workdir, "/tmp");
        assert_eq!(compactor.model, "test-model");
        assert_eq!(compactor.threshold, 50000);
        assert_eq!(compactor.keep_recent, 3);

        // Verify tools
        let tools = compactor.tools.as_array().unwrap();
        assert_eq!(tools.len(), 5);
        assert_eq!(tools[4]["name"], "compact");
    }
}
