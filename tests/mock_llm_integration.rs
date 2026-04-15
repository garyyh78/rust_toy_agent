use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use tokio::time::{timeout, Duration};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn bin() -> Command {
    Command::cargo_bin("rust_toy_agent").unwrap()
}

#[tokio::test]
async fn mock_server_returns_tool_use_sequence() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "sk-test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "message",
            "id": "msg_test123",
            "role": "assistant",
            "content": [
                {
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "bash",
                    "input": {"command": "echo hello"}
                }
            ],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 20}
        })))
        .mount(&mock_server)
        .await;

    let result = timeout(Duration::from_secs(10), async {
        let client =
            rust_toy_agent::llm_client::AnthropicClient::new("sk-test-key", &mock_server.uri());
        let messages = vec![json!({"role": "user", "content": "say hello"})];
        client
            .create_message("claude-sonnet-4-20250514", None, &messages, None, 1024)
            .await
    })
    .await;

    assert!(result.is_ok(), "Request should complete");
    let response = result.unwrap();
    assert!(response.is_ok(), "Should not error");
    let json_resp = response.unwrap();
    assert!(json_resp.get("content").is_some());
}

#[tokio::test]
async fn mock_server_429_then_200_shows_retry() {
    let mock_server = MockServer::start().await;

    let call_counter = std::sync::atomic::AtomicU32::new(0);
    let counter_for_closure = std::sync::Arc::new(call_counter);
    let counter_for_closure_inner = counter_for_closure.clone();

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "sk-test-key"))
        .respond_with(move |_req: &wiremock::Request| {
            let value = counter_for_closure_inner.clone();
            let count = value.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if count == 0 {
                ResponseTemplate::new(429).set_body_json(json!({
                    "type": "error",
                    "error": {
                        "type": "rate_limit_error",
                        "message": "Rate limited"
                    }
                }))
            } else {
                ResponseTemplate::new(200).set_body_json(json!({
                    "type": "message",
                    "id": "msg_after_retry",
                    "role": "assistant",
                    "content": [
                        {
                            "type": "tool_use",
                            "id": "toolu_2",
                            "name": "read",
                            "input": {"path": "/tmp/test.txt"}
                        }
                    ],
                    "model": "claude-sonnet-4-20250514",
                    "stop_reason": "tool_use",
                    "usage": {"input_tokens": 10, "output_tokens": 20}
                }))
            }
        })
        .mount(&mock_server)
        .await;

    let final_counter = counter_for_closure.clone();

    let result = timeout(Duration::from_secs(15), async {
        let client =
            rust_toy_agent::llm_client::AnthropicClient::new("sk-test-key", &mock_server.uri())
                .with_max_retries(3);
        let messages = vec![json!({"role": "user", "content": "list files"})];
        client
            .create_message("claude-sonnet-4-20250514", None, &messages, None, 1024)
            .await
    })
    .await;

    assert!(result.is_ok(), "Request should complete");
    let response = result.unwrap();
    assert!(response.is_ok(), "Should succeed after retry");
    let json_resp = response.unwrap();
    assert!(json_resp.get("content").is_some(), "Should have content");
    assert!(
        final_counter.load(std::sync::atomic::Ordering::SeqCst) >= 2,
        "Should have retried at least once"
    );
}

#[test]
fn help_works_with_no_api_key() {
    bin()
        .env("ANTHROPIC_API_KEY", "")
        .arg("--help")
        .assert()
        .success()
        .stderr(predicate::str::contains("Usage"));
}
