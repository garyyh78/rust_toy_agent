# Chapter 7: Robust LLM I/O — Never Trust the Network

> "The first rule of distributed systems is: don't." — *Rich Hickey*

## Introduction

Every agent is a distributed system. The harness is one process, the LLM is another, they communicate over TCP/TLS. Every call can fail: dropped packet, rate limit, API deploy mid-request, cloud hiccup, Wi-Fi disconnect. The question is not *if* something fails — it is *when* and *how gracefully* your agent recovers.

This chapter covers `src/llm_client.rs`: classified errors, exponential backoff with jitter, and idempotency.

## 7.1 The Client Type

```rust
pub struct AnthropicClient {
    pub api_key: String,
    pub base_url: String,
    client: reqwest::Client,
    max_retries: u32,
}
```

Four fields. The third is a **reusable HTTP client** — a connection pool that holds open TLS sockets across requests. Creating one is expensive; reusing one is cheap. Build a new client per call and you pay a TCP+TLS handshake on every turn.

The fourth field — `max_retries` — deserves its own section.

## 7.2 Timeouts: The Outer Envelope

```rust
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
```

Two timeouts, not one:

- **`CONNECT_TIMEOUT`** — bounds how long we wait for TCP handshake and TLS setup. 15 seconds is generous.
- **`REQUEST_TIMEOUT`** — bounds total time from "start request" to "received full response." 300 seconds is five minutes, long enough for large multi-tool response but short enough that stuck request doesn't hang forever.

The rookie mistake: set one timeout (say, 60 seconds for "everything") and discover it sometimes cuts off legitimate slow responses and sometimes lets broken connections hang for a minute.

## 7.3 Transient vs Fatal: The Most Important Distinction

When an HTTP call fails, there are two very different things it could mean:

1. **Transient** — likely to work if I try again. Network blip, rate limit, server restart.
2. **Fatal** — will not work if I try again. Invalid API key, malformed request, nonexistent model.

```rust
enum SendError {
    Transient(String),
    Fatal(String),
}
```

Classification logic:

```rust
if status.is_success() {
    return serde_json::from_str(&text)
        .map_err(|e| SendError::Fatal(format!("parse: {e}")));
}
let msg = format!("Anthropic API error {status}: {text}");
if status.as_u16() == 429 || status.is_server_error() {
    Err(SendError::Transient(msg))
} else {
    Err(SendError::Fatal(msg))
}
```

Three buckets:

- **2xx — success** — parse failures are **fatal**
- **429 (rate limit) or 5xx — transient** — server literally said "try again later"
- **Everything else (4xx) — fatal** — 400 malformed, 401 wrong key, 403 no access

Connection-level failures are transient unconditionally:

```rust
.send()
.await
.map_err(|e| {
    SendError::Transient(format!("HTTP request failed: {e}"))
})?;
```

A request that never reached the server is always a candidate for retry.

## 7.4 The Retry Loop

```rust
pub async fn send_body(&self, body: &Json) -> Result<Json, String> {
    let url = format!("{}/v1/messages", self.base_url);
    let mut backoff = INITIAL_BACKOFF;    // 1 second
    let mut attempt: u32 = 0;

    loop {
        match self.send_once(&url, body).await {
            Ok(json) => return Ok(json),
            Err(SendError::Transient(msg)) if attempt < self.max_retries => {
                tokio::time::sleep(backoff + jitter(backoff)).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);  // cap at 30 seconds
                attempt += 1;
            }
            Err(SendError::Transient(msg)) | Err(SendError::Fatal(msg)) => {
                return Err(msg)
            }
        }
    }
}
```

Five observations:

1. **Happy path is first branch** — 99% of calls succeed on attempt one
2. **Transient errors only retry while `attempt < max_retries`** — don't retry forever
3. **Backoff doubles** — 1s, 2s, 4s, 8s, 16s, 30s. By third retry, server has breathing room
4. **Jitter added to sleep** — see next section
5. **Fatal errors return immediately** — no sleep, no attempt counter

## 7.5 Jitter: The Weird Trick That Works

```rust
fn jitter(base: Duration) -> Duration {
    let fraction = RNG.with(|rng| {
        let mut rng = rng.borrow_mut();
        rng.gen_range(0..base.as_millis() as u64 / 4)
    });
    Duration::from_millis(fraction)
}
```

Random milliseconds between 0 and `base / 4`. Why?

Imagine 100 agents hit a rate limit at the same moment. They all get 429. They all wait 1 second. They all retry at once. They all get rate-limited again. This is **thundering herd** — a five-second hiccup becomes a five-minute outage.

Adding random component *desynchronises* retries. Agent A waits 1.05s, B waits 1.23s, C waits 0.91s. Server can handle them one at a time.

`base / 4` adds 25% of base backoff — enough to scatter the herd without making backoff feel lumpy. Amazon's recommendation is `rand(0, base)` — full jitter. What does NOT work is zero jitter.

## 7.6 Idempotency

If network drops response *after* server processed request, retry runs request a second time. If request has side effects (create resource, charge credit card), you just did side effect twice.

LLM API calls are special: they're *almost* idempotent. Same input produces almost-the-same output. Server bills you twice but nothing else goes wrong.

**Tool calls are the opposite.** `run_bash("rm tmp.txt")` is NOT idempotent. Retrying deletes already-deleted file.

`rust_toy_agent` sidesteps elegantly: **tool calls are not retried at the harness level**. When tool fails, failure is returned to model as tool_result with error message, and model decides whether to retry.

**General rule:** retry at the layer that understands idempotency. Infrastructure retries (network, DNS, TCP) at HTTP layer. Semantic retries (try that edit again) at model layer.

## 7.7 The Escape Hatch: `with_max_retries(0)`

```rust
#[must_use]
pub fn with_max_retries(mut self, retries: u32) -> Self {
    self.max_retries = retries;
    self
}
```

Looks trivial. It is essential.

- **Tests use it** — failing request doesn't sit in exponential backoff for 30 seconds. Without this, test suite takes 10× as long.
- **Interactive use sometimes benefits from fast-fail** — if API is obviously down, surface error in 2 seconds, not 2 minutes.
- **Chained clients need to pick their retry budget** — subagent retried by parent shouldn't also retry internally.

Every constant in retry machinery should be a knob. Timeouts, retry counts, backoff multipliers — all configurable.

## 7.8 Structured Logging at Failures

```rust
tracing::warn!(
    msg = %msg,
    attempt = %attempt,
    max_retries = %self.max_retries,
    backoff_ms = ?backoff,
    "transient error, retrying"
);
```

Structured fields, not string formatting. In production log pipeline, those fields become columns in a search index. String logs are searchable only by substring.

Every failure path should emit structured logs. From day one.

---

**Next:** Chapter 8 — Prompt Engineering in Code