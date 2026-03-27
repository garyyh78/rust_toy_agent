//! client.rs - Anthropic API client
//!
//! Wraps reqwest to talk to the Anthropic Messages API.

use serde_json::Value as Json;
use std::env;

pub struct AnthropicClient {
    pub api_key: String,
    pub base_url: String,
    client: reqwest::Client,
}

impl AnthropicClient {
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

    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: base_url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn create_message(
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
        // When env vars are not set, from_env should still construct
        // (api_key will be empty, base_url will be default)
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
        let body = AnthropicClient::build_request_body("claude-sonnet-4-20250514", None, &messages, None, 1024);
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
