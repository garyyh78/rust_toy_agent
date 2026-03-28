//! client.rs - Anthropic API client
//!
//! Wraps reqwest to talk to the Anthropic Messages API.
//!
//! Relationship diagram:
//!
//!   ┌───────────────────────────────────────────────────┐
//!   │              AnthropicClient                      │
//!   ├───────────────────────────────────────────────────┤
//!   │  from_env()  ── reads ANTHROPIC_API_KEY           │
//!   │                reads ANTHROPIC_BASE_URL            │
//!   │                                                   │
//!   │  new()       ── explicit credentials              │
//!   │                                                   │
//!   │  build_request_body() ── pure JSON builder        │
//!   │      ┌──────────────────────────────────┐         │
//!   │      │ { model, system?, messages,      │         │
//!   │      │   tools?, max_tokens }           │         │
//!   │      └──────────────────────────────────┘         │
//!   │                                                   │
//!   │  create_message() ── async HTTP POST              │
//!   │      │                                            │
//!   │      ├─ build_request_body()                      │
//!   │      ├─ POST /v1/messages                         │
//!   │      ├─ check status code                         │
//!   │      └─ parse JSON response                       │
//!   └───────────────────────────────────────────────────┘
//!
//! Used by: agent_loop.rs (create_message)
//! Tested by: 7 unit tests (build_request_body variants)

use serde_json::Value as Json;
use std::env;

/// Holds API credentials and a shared HTTP client for reuse across requests.
pub struct AnthropicClient {
    pub api_key: String,
    pub base_url: String,
    client: reqwest::Client,
}

impl AnthropicClient {
    /// Build a client from environment variables (ANTHROPIC_API_KEY, ANTHROPIC_BASE_URL).
    /// Falls back to the official Anthropic endpoint if no base URL is set.
    pub fn from_env() -> Self {
        let base_url = env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
        let api_key = env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        Self {
            api_key,
            base_url,
            client: reqwest::Client::new(),
        }
    }

    /// Build a client with explicit credentials (useful for tests or custom setups).
    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: base_url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Send a messages request to the Anthropic API.
    /// Returns the full JSON response on success, or an error string on failure.
    pub async fn create_message(
        &self,
        model: &str,
        system: Option<&str>,
        messages: &[Json],
        tools: Option<&Json>,
        max_tokens: u32,
    ) -> Result<Json, String> {
        // Build the request body, omitting optional fields when empty
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

        // Send the HTTP POST with required Anthropic headers
        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        // Check for API errors
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {e}"))?;
        if !status.is_success() {
            return Err(format!("Anthropic API error {status}: {text}"));
        }
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse API response: {e}"))
    }

    /// Build the request body JSON without sending it.
    /// Useful for inspecting or logging what would be sent.
    pub fn build_request_body(
        model: &str,
        system: Option<&str>,
        messages: &[Json],
        tools: Option<&Json>,
        max_tokens: u32,
    ) -> Json {
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
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_env_defaults() {
        let client = AnthropicClient::from_env();
        assert_eq!(client.base_url, "https://api.anthropic.com");
    }

    #[test]
    fn test_new_with_args() {
        let client = AnthropicClient::new("sk-test", "https://custom.api.com");
        assert_eq!(client.api_key, "sk-test");
        assert_eq!(client.base_url, "https://custom.api.com");
    }

    #[test]
    fn test_build_request_body_minimal() {
        let messages = vec![serde_json::json!({"role": "user", "content": "hi"})];
        let body = AnthropicClient::build_request_body(
            "claude-sonnet-4-20250514",
            None,
            &messages,
            None,
            1024,
        );
        assert_eq!(body["model"], "claude-sonnet-4-20250514");
        assert_eq!(body["max_tokens"], 1024);
        assert!(body.get("system").is_none());
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn test_build_request_body_with_system() {
        let messages = vec![serde_json::json!({"role": "user", "content": "hi"})];
        let body = AnthropicClient::build_request_body(
            "claude-sonnet-4-20250514",
            Some("You are helpful."),
            &messages,
            None,
            1024,
        );
        assert_eq!(body["system"], "You are helpful.");
    }

    #[test]
    fn test_build_request_body_with_tools() {
        let messages = vec![serde_json::json!({"role": "user", "content": "hi"})];
        let tools = serde_json::json!([{"name": "bash", "description": "Run command"}]);
        let body = AnthropicClient::build_request_body(
            "claude-sonnet-4-20250514",
            None,
            &messages,
            Some(&tools),
            1024,
        );
        assert!(body["tools"].is_array());
        assert_eq!(body["tools"][0]["name"], "bash");
    }

    #[test]
    fn test_build_request_body_empty_system_omitted() {
        let messages = vec![serde_json::json!({"role": "user", "content": "hi"})];
        let body = AnthropicClient::build_request_body(
            "claude-sonnet-4-20250514",
            Some(""),
            &messages,
            None,
            1024,
        );
        assert!(body.get("system").is_none());
    }

    #[test]
    fn test_build_request_body_empty_tools_omitted() {
        let messages = vec![serde_json::json!({"role": "user", "content": "hi"})];
        let tools = serde_json::json!([]);
        let body = AnthropicClient::build_request_body(
            "claude-sonnet-4-20250514",
            None,
            &messages,
            Some(&tools),
            1024,
        );
        assert!(body.get("tools").is_none());
    }
}
