# Chapter 7: Robust LLM I/O — Never Trust the Network

> "The first rule of distributed systems is: don't." — Rich Hickey

Every agent is a distributed system. The harness is one process,
the LLM is another, they communicate over TCP/TLS and HTTPS, and
every single call can fail for reasons that have nothing to do
with your code: a dropped packet, a rate-limited tenant, an API
deploy mid-request, a cloud provider hiccup, a Wi-Fi disconnect
on the laptop the whole thing runs on. The question is not *if*
something in that chain will fail — it is *when*, and *how
gracefully* your agent recovers.

This chapter is about the code that takes that seriously. The
file is `src/llm_client.rs`, and the three ideas inside it —
classified errors, exponential backoff with jitter, and
idempotency — are the whole of robust LLM I/O. They are ancient
distributed-systems lore, dressed up for a modern API.

## 7.1 The Client Type

The client itself is small:

```rust
pub struct AnthropicClient {
    pub api_key: String,
    pub base_url: String,
    client: reqwest::Client,
    max_retries: u32,
}
```

Four fields. The first two are what everyone expects. The third
is a *reusable HTTP client* — a connection pool that holds open
TLS sockets across requests. Creating one of these is expensive
(TLS handshake, DNS resolution); reusing one is cheap. Every
modern HTTP library has the same pattern, and every agent harness
should share a single client across the whole session. If you
build a new client per call, you pay a TCP+TLS handshake on every
turn, and a busy agent spends a noticeable fraction of wall-clock
on handshakes.

The fourth field — `max_retries` — deserves its own section.

## 7.2 Timeouts: The Outer Envelope

Two constants at the top of the file:

```rust
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
```

Two timeouts, not one. The difference matters.

* **`CONNECT_TIMEOUT`** bounds how long we wait for the TCP
  handshake and TLS setup. 15 seconds is generous; if you can't
  even *connect* in 15 seconds, something is wrong and retrying
  quickly is the right call.
* **`REQUEST_TIMEOUT`** bounds the total time from "start the
  request" to "received the full response." 300 seconds is five
  minutes, which is long enough for the model to produce a large
  multi-tool response but short enough that a genuinely stuck
  request doesn't hang the loop forever.

The rookie mistake is to set one timeout — say, `60 seconds` for
"everything" — and discover that it sometimes cuts off a
legitimately slow LLM response and sometimes lets a broken
connection hang for a minute before retrying. Two timeouts are
two dials, and you want both of them.

Modern APIs add a third layer: **streaming**. With streaming, you
can set a *read* timeout — "the server must send a byte every N
seconds" — which lets you detect a stalled mid-stream connection
without aborting a still-productive one. `rust_toy_agent` does
not stream, so it does not need this, but your production agent
probably will.

## 7.3 Transient vs Fatal: The Most Important Distinction

When an HTTP call fails, there are two very different things the
failure could mean:

1. **Transient:** something that is likely to work if I try again.
   Network blip, rate limit, server restart, congestion.
2. **Fatal:** something that will not work if I try again.
   Invalid API key, malformed request, nonexistent model name,
   permission denied.

Every retry strategy begins with this classification. Retry a
fatal error and you burn money and time accomplishing nothing.
Fail fast on a transient error and you give up where a second
attempt would have succeeded. The code:

```rust
enum SendError {
    Transient(String),
    Fatal(String),
}
```

Two variants, and that's the whole classifier type. It lives only
inside `llm_client.rs` — the outside world never sees the
distinction, because by the time an error leaves the module it
has already been retried (if transient) or propagated (if fatal).

The classification logic lives in `send_once`:

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

* **2xx — success.** Try to parse the body. Parse failures are
  **fatal** — if the server returned a malformed response, retrying
  will just produce another malformed response.
* **429 (rate limit) or 5xx (server error) — transient.** The
  server literally told us "try again later."
* **Everything else (4xx) — fatal.** 400 means our request was
  malformed; 401 means our key is wrong; 403 means our account
  does not have access; 404 means the model name is wrong.
  Retrying any of these is pointless and, in the case of 401 and
  403, actively harmful (you may trip rate-limiting or
  account-suspension heuristics on the provider side).

One exception is worth knowing about: **connection-level failures
are transient, unconditionally**. The retry path handles them
here:

```rust
.send()
.await
.map_err(|e| {
    SendError::Transient(format!("HTTP request failed: {e}"))
})?;
```

A request that never reached the server is always a candidate for
retry. The network might have dropped the packet; a partial
handshake might have timed out. We did not get far enough to
learn anything about whether the *server* would have processed
it.

## 7.4 The Retry Loop

Now the full retry loop:

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

Five things to notice:

1. **The happy path is the first branch.** Ninety-nine percent of
   calls succeed on attempt one; the code should make that case
   read as a single line.
2. **Transient errors only retry while `attempt < max_retries`.**
   Once the budget is exhausted, a transient error is promoted to
   a returned error. Do not retry forever. A forever-retry is how
   agents silently burn $500 overnight.
3. **The backoff doubles after each attempt**, capped at
   `MAX_BACKOFF = 30 seconds`. 1s, 2s, 4s, 8s, 16s, 30s. By the
   third retry you are giving the server real breathing room.
4. **Jitter is added to the sleep.** More on this in §7.5 — it
   is the most important single trick in distributed retries.
5. **Fatal errors return immediately.** No sleep, no attempt
   counter, no discussion.

The loop compiles down to maybe fifty machine instructions. The
cleverness is in what it *doesn't* do. It does not retry fatal
errors. It does not retry forever. It does not retry with a
constant delay. It does not retry without jitter. Each of those
mistakes is a production outage waiting to happen, and each of
them is an easy mistake to make.

## 7.5 Jitter: The Weird Trick That Works

Look at `jitter`:

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

Imagine 100 agents all hit a rate limit at the same moment. They
all get a 429. They all wait exactly 1 second. They all retry at
once. They all get rate-limited again. They all wait exactly
2 seconds. They all retry at once. This is called the
**thundering herd**, and it turns a five-second hiccup into a
five-minute outage. The server cannot drain the queue because
every agent is retrying in lockstep.

Adding a random component to each sleep *desynchronises* the
retries. Agent A waits 1.05 seconds, agent B waits 1.23 seconds,
agent C waits 0.91 seconds, and their retry attempts spread out
across a window. The server can now handle them one at a time.

The amount of jitter does not matter as much as the presence of
any jitter at all. `base / 4` is a reasonable default: it adds up
to 25% of the base backoff, enough to scatter the herd without
making the backoff feel lumpy to a human watching logs. Amazon's
published recommendation ("full jitter") is `rand(0, base)` —
even more aggressive — and also works fine. What does *not* work
is zero jitter.

Jitter is the second-most-underused trick in distributed systems.
The most-underused is...

## 7.6 Idempotency

If the network drops the response *after* the server processed
the request, your retry will run the request a second time. If
the request has side effects — creating a resource, charging a
credit card, sending an email — you have just done the side
effect twice.

LLM API calls are special: they're *almost* idempotent. The same
input produces almost-the-same output (temperature > 0 adds
randomness, but the shape is the same). The server bills you
twice, but nothing else goes wrong. This is why `rust_toy_agent`
can afford to retry LLM calls blindly.

*Tool calls* are the opposite. `run_bash("rm tmp.txt")` is not
idempotent. Retrying it after a transient failure will try to
delete a file that is already gone, and `rm` will complain.
Worse, `run_bash("curl -X POST billing.example.com/charge?amount=100")`
is *dangerously* non-idempotent.

`rust_toy_agent` sidesteps this elegantly: **tool calls are not
retried at the harness level**. When a tool fails, the failure
is returned to the model as a `tool_result` with an error message,
and the model decides whether to retry. The model is better than
the harness at knowing whether a retry is safe, because the model
understands the semantics of what it was trying to do.

The general rule: **retry at the layer that understands
idempotency**. Infrastructure retries (network, DNS, TCP) happen
at the HTTP layer. Semantic retries (try that edit again) happen
at the model layer. Do not mix them.

## 7.7 The Escape Hatch: `with_max_retries(0)`

```rust
#[must_use]
pub fn with_max_retries(mut self, retries: u32) -> Self {
    self.max_retries = retries;
    self
}
```

A small builder method that lets a caller turn retries *off*. It
looks trivial. It is essential.

* **Tests use it.** The test suite calls
  `AnthropicClient::new("sk-fake", "http://127.0.0.1:1")
  .with_max_retries(0)` so a failing request doesn't sit in
  exponential backoff for 30 seconds before the test declares
  failure. Without this knob, the test suite would take ten
  times as long and developers would stop running it.
* **Interactive use sometimes benefits from fast-fail.** If a
  human is watching and the API is obviously down, you want to
  surface the error in two seconds, not two minutes.
* **Chained clients need to pick their retry budget.** A
  subagent that is itself being retried by a parent shouldn't
  also retry internally — that gives you N × M attempts, and
  the total sleep becomes exponential.

The pattern is: **every constant in your retry machinery should
be a knob**. Timeouts, retry counts, backoff multipliers — all
configurable, with sensible defaults. You will discover at 2am
during an incident that you want to tweak one of them, and at
2am is not the time to be rebuilding the binary.

## 7.8 Structured Logging at Failures

One last observation. The retry path logs like this:

```rust
tracing::warn!(
    msg = %msg,
    attempt = %attempt,
    max_retries = %self.max_retries,
    backoff_ms = ?backoff,
    "transient error, retrying"
);
```

Structured fields, not string formatting. When the agent is
running in a production log pipeline, those fields become columns
in a search index, and you can ask questions like *"how often do
our retries exceed 3 attempts?"* or *"which base URLs have the
highest 5xx rate?"* A string log like `"transient error 429,
retrying, attempt 2 of 3"` is searchable only by substring —
which is fine until you have 100,000 messages a day.

Every failure path in your agent should emit structured logs.
Not eventually, not when you scale up — from day one. Chapter 10
returns to this in the context of observability.

## 7.9 Exercises

1. Trace what happens when a 401 is returned: which function
   classifies it, how far does the error propagate, and what
   does the caller see? Compare to a 429.

2. Change `INITIAL_BACKOFF` to 10 seconds and run the test that
   uses `with_max_retries(0)`. Does the test slow down? Why
   not?

3. The retry loop sleeps with `tokio::time::sleep`. What would
   go wrong if you used `std::thread::sleep` instead, given the
   async context? (Hint: Chapter 8.)

4. `jitter` uses up to 25% of the base delay. Propose a
   different distribution — "full jitter" from §7.5, or
   "decorrelated jitter," or your own. Make a case for yours.

5. The comment on `send_once` says connection failures are
   transient. What about DNS failures? If the API's hostname
   doesn't resolve, should you retry? Defend your answer.

In Chapter 8 we pull back to look at prompt engineering not as
a model-interaction skill but as a *coding* skill — the part of
the harness that lives in strings and shapes the model's
behaviour without any code at all.
