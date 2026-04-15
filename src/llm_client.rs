//! `llm_client.rs` - Anthropic API client
//!
//! Wraps reqwest to talk to the Anthropic Messages API.
//!
//! Relationship diagram:
//!
//!   ┌───────────────────────────────────────────────────┐
//!   │              `AnthropicClient`                      │
//!   ├───────────────────────────────────────────────────┤
//!   │  `from_env()`  ── reads `ANTHROPIC_API_KEY`           │
//!   │                reads `ANTHROPIC_BASE_URL`            │
//!   │                                                   │
//!   │  `new()`       ── explicit credentials              │
//!   │                                                   │
//!   │  `build_request_body()` ── pure JSON builder        │
//!   │      ┌──────────────────────────────────┐         │
//!   │      │ { model, system?, messages,      │         │
//!   │      │   tools?, `max_tokens` }           │         │
//!   │      └──────────────────────────────────┘         │
//!   │                                                   │
//!   │  `create_message()` ── async HTTP POST              │
//!   │      │                                            │
//!   │      ├─ `build_request_body()`                      │
//!   │      ├─ POST /v1/messages                         │
//!   │      ├─ check status code                         │
//!   │      └─ parse JSON response                       │
//!   └───────────────────────────────────────────────────┘
//!
//! Used by: `agent_loop.rs` (`create_message`)
//! Tested by: 7 unit tests (`build_request_body` variants)

use rand::{Rng, SeedableRng};
use serde_json::Value as Json;
use std::env;
use std::time::Duration;

/// Total-request timeout: caps how long a single call waits for the API to finish.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
/// Connection timeout: how long we wait for TCP/TLS handshake before giving up.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Default max retry attempts on transient failures (429, 5xx, network errors).
const DEFAULT_MAX_RETRIES: u32 = 3;
/// Initial backoff delay; doubled after each retry up to `MAX_BACKOFF`.
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
/// Cap on per-retry sleep so one run doesn't block for minutes.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Build a reqwest client with sensible timeouts. Falls back to the default
/// client if builder fails (should not happen with static values).
fn build_http_client() -> reqwest::Client {
    reqwest::ClientBuilder::new()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Classified send error. Transient → retry; Fatal → propagate immediately.
enum SendError {
    Transient(String),
    Fatal(String),
}

thread_local! {
    static RNG: std::cell::RefCell<rand::rngs::StdRng> = std::cell::RefCell::new(
        rand::rngs::StdRng::from_seed([
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos() as u8,
            17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
        ])
    );
}

fn jitter(base: Duration) -> Duration {
    let fraction = RNG.with(|rng| {
        let mut rng = rng.borrow_mut();
        rng.gen_range(0..base.as_millis() as u64 / 4)
    });
    Duration::from_millis(fraction)
}

/// Holds API credentials and a shared HTTP client for reuse across requests.
pub struct AnthropicClient {
    pub api_key: String,
    pub base_url: String,
    client: reqwest::Client,
    max_retries: u32,
}

impl AnthropicClient {
    /// Build a client from environment variables (`ANTHROPIC_API_KEY`, `ANTHROPIC_BASE_URL`).
    /// Falls back to the official Anthropic endpoint if no base URL is set.
    pub fn from_env() -> Self {
        let base_url = env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
        let api_key = env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        Self {
            api_key,
            base_url,
            client: build_http_client(),
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }

    /// Build a client with explicit credentials (useful for tests or custom setups).
    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: base_url.to_string(),
            client: build_http_client(),
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }

    /// Override the retry budget (default 3). Zero disables retries entirely —
    /// useful for tests that want fast-fail behavior.
    #[must_use]
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
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
        let body = Self::build_request_body(model, system, messages, tools, max_tokens);
        self.send_body(&body).await
    }

    /// Send a pre-built request body to the API with retry/backoff on
    /// transient failures (429, 5xx, network errors). 4xx client errors are
    /// surfaced immediately since they will not succeed on retry.
    pub async fn send_body(&self, body: &Json) -> Result<Json, String> {
        let url = format!("{}/v1/messages", self.base_url);
        let mut backoff = INITIAL_BACKOFF;
        let mut attempt: u32 = 0;

        loop {
            let attempt_result = self.send_once(&url, body).await;
            match attempt_result {
                Ok(json) => return Ok(json),
                Err(SendError::Transient(msg)) if attempt < self.max_retries => {
                    tracing::warn!(
                        msg = %msg,
                        attempt = %attempt,
                        max_retries = %self.max_retries,
                        backoff_ms = ?backoff,
                        "transient error, retrying"
                    );
                    tokio::time::sleep(backoff + jitter(backoff)).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                    attempt += 1;
                }
                Err(SendError::Transient(msg)) | Err(SendError::Fatal(msg)) => return Err(msg),
            }
        }
    }

    /// Execute exactly one HTTP POST. Distinguishes transient vs fatal errors
    /// so the retry loop knows when to back off.
    async fn send_once(&self, url: &str, body: &Json) -> Result<Json, SendError> {
        let resp = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| {
                // Network/timeout/connect failures are transient by nature.
                SendError::Transient(format!("HTTP request failed: {e}"))
            })?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| SendError::Transient(format!("Failed to read response body: {e}")))?;

        if status.is_success() {
            return serde_json::from_str(&text)
                .map_err(|e| SendError::Fatal(format!("Failed to parse API response: {e}")));
        }

        let msg = format!("Anthropic API error {status}: {text}");
        if status.as_u16() == 429 || status.is_server_error() {
            Err(SendError::Transient(msg))
        } else {
            Err(SendError::Fatal(msg))
        }
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
            if t.as_array().is_some_and(|a| !a.is_empty()) {
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

    // -- Error handling tests (the 400 panic fix) --

    #[tokio::test]
    async fn test_create_message_returns_err_on_bad_url() {
        // Point at a URL that will fail to connect. Retries disabled so the
        // test fails fast instead of backing off through 3 attempts.
        let client = AnthropicClient::new("sk-fake", "http://127.0.0.1:1").with_max_retries(0);
        let messages = vec![serde_json::json!({"role": "user", "content": "hi"})];
        let result = client
            .create_message("claude-sonnet-4-20250514", None, &messages, None, 1024)
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("HTTP request failed"),
            "Expected HTTP error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_create_message_returns_err_on_api_error() {
        // Use a real URL that returns 401 (no valid API key)
        let client = AnthropicClient::new("sk-fake-key", "https://api.anthropic.com");
        let messages = vec![serde_json::json!({"role": "user", "content": "hi"})];
        let result = client
            .create_message("claude-sonnet-4-20250514", None, &messages, None, 1024)
            .await;
        assert!(result.is_err(), "Should return Err, not panic");
        let err = result.unwrap_err();
        assert!(
            err.contains("Anthropic API error"),
            "Expected API error message, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_create_message_err_contains_status_code() {
        let client = AnthropicClient::new("bad-key", "https://api.anthropic.com");
        let messages = vec![serde_json::json!({"role": "user", "content": "test"})];
        let result = client
            .create_message("claude-sonnet-4-20250514", None, &messages, None, 256)
            .await;
        let err = result.unwrap_err();
        // Should contain a status code like "401" or "400"
        assert!(
            err.contains("401") || err.contains("400") || err.contains("403"),
            "Expected status code in error, got: {err}"
        );
    }
}
