# Chapter 10: Observability and Shipping

> "In theory there is no difference between theory and practice. In practice there is." — *Yogi Berra*

## Introduction

Nine chapters in, `rust_toy_agent` is a working coding agent. It holds conversation, dispatches tools, recovers from flaky networks, plans with a todo list, spawns subagents, runs shell commands in background. It is a demo on your laptop. What it is NOT is a tool other people can safely point at their repositories.

The gap is not in cleverness — the clever parts are done. The gap is in **operability**: the unglamorous plumbing that turns a thing that works on your machine into a thing you can hand to someone else, debug from a bug report, diff against yesterday's behavior, and unwind cleanly when it makes a mess.

Four pieces of plumbing: session logs, metrics, worktree isolation, event buses.

## 10.1 Two Readers, Two Formats

Observability writes for two different readers who want different things:

**Reader one: human.** Wants chronological transcript, human-readable, colourised when it helps, skimmable when session is long. Prose fine. Timestamps nice. Line wrapping matters.

**Reader two: machine** — dashboard, pipeline, script that rolls up numbers across sessions. Does NOT care about formatting; wants structured records with stable keys, one per line. Prose actively hostile. Timestamps must be machine-readable.

Good observability ships BOTH. Bad observability tries to ship one and hopes other reader will cope.

## 10.2 The Session Logger

```rust
pub struct SessionLogger {
    file: Option<File>,
}
```

One field: optional file handle. Job is to fan out calls to two sinks — stderr and file — and let caller pretend there is only one sink.

Methods in two flavors. Free functions for stderr-only:

```rust
pub fn log_section(title: &str) {
    eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    eprintln!(" {title}");
    eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
}

pub fn log_info(label: &str, value: &str) {
    eprintln!("  {label:<12} {value}");
}
```

And methods that do stderr write AND append timestamped line to log file:

```rust
pub fn log_section(&mut self, title: &str) {
    log_section(title);
    self.write_file(&format!("[{}] === {} ===", Self::timestamp(), title));
}
```

Three details:

- **Stderr, not stdout** — every diagnostic uses `eprintln!`, never `println!`. Agent's REPL puts model's prose on stdout; everything else goes on stderr. Pick a side for logs and never cross.

- **Colours on stderr, plain text in file** — terminal renders stderr beautifully; text editor renders file readably. Ship colours to file and you see `\x1b[31m` everywhere. Strip presentation on way to durable storage.

- **Timestamps on file, not on stderr** — stderr is "now," timestamp is clutter. File is "then," without timestamp every line is unmoored.

## 10.3 Logging User and Agent Turns

```rust
pub fn log_user_input(&mut self, input: &str) {
    self.write_file(&format!("[{}] USER > {input}", Self::timestamp()));
}

pub fn log_agent_response(&mut self, text: &str) {
    for line in text.lines() {
        self.write_file(&format!("[{}] AGENT < {line}", Self::timestamp()));
    }
}
```

These are file-only. Terminal already showed user their own input and agent's response. File is only place that needs both.

`USER >` and `AGENT <` are more than decoration. They give you the affordance that matters when debugging: grep for every user message with `grep 'USER >'` and every agent response with `grep 'AGENT <'`.

**A log format is a contract with future-you.** Pick marker strings that are unambiguous, short, and easy to grep. Then do not change them.

## 10.4 API Request and Response Logging

```rust
pub fn log_api_request(&mut self, body: &serde_json::Value) {
    self.write_file(&format!(
        "[{}] ── API REQUEST ──────────────────────────────",
        Self::timestamp()
    ));
    for line in serde_json::to_string_pretty(body)
        .unwrap_or_default()
        .lines()
    {
        self.write_file(&format!("[{}]   {line}", Self::timestamp()));
    }
}
```

Every line of pretty-printed JSON gets its own timestamp. That looks wasteful — why not one timestamp at top? — and then one day you're debugging 4 a.m. incident where agent hangs mid-request and request body was 40 KB and only half was written before process died. Line-level timestamps tell you exactly how far write got.

Logging full API bodies is expensive. Do it anyway, at least in development and during incidents.

## 10.5 Log Rotation

```rust
const MAX_LOG_FILES: usize = 20;

fn prune_old_logs(dir: &Path) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("session_"))
        .collect();
    entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    while entries.len() > MAX_LOG_FILES {
        let oldest = entries.remove(0);
        let _ = std::fs::remove_file(oldest.path());
    }
    Ok(())
}
```

Keep 20 most recent session files, delete rest. No size-based rotation. No compression. No database. Twenty is enough to find yesterday's problem without logs dir being huge.

The lesson: **build retention behavior at all**. Agent with growing log directory and no pruning is pager alert waiting to happen.

Pruning runs when new `SessionLogger` is created. Lazy, opportunistic schedule — old logs don't get pruned until next session starts.

## 10.6 Metrics: The Machine-Facing Side

```rust
#[derive(Serialize)]
pub struct RoundMetrics {
    pub timestamp: String,
    pub session_id: String,
    pub round: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub wall_ms: u64,
    pub tool_calls: u32,
    pub retries: u32,
    pub host: &'static str,
}

pub fn emit(path: &Path, m: &RoundMetrics) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{}", serde_json::to_string(m).unwrap())?;
    Ok(())
}
```

Twenty-six lines and a format. What machine-facing telemetry looks like before you have scale problems.

Four observations:

- **One record per round, append-only** — JSON Lines format, newline-delimited JSON, survives truncation

- **Schema is narrow and fixed** — eight fields, clear non-overlapping purpose. Model name, prompt text NOT here. Metrics are for aggregation; context belongs in session log.

- **`&'static str` for the host** — compile-time constant, not heap allocation. Build-time property.

- **Emit function is transactional-enough** — open, append, close. JSON Lines records are whole or absent.

## 10.7 What Metrics to Collect

Every field answers a real question someone will ask about the agent:

- **`input_tokens`, `output_tokens`** — "what is this costing us?"
- **`wall_ms`** — "is agent getting slower?"
- **`tool_calls`** — "is agent working or thinking?" Distribution shifts when something wrong.
- **`retries`** — "is network healthy?" If retry counts spike, something upstream degraded.
- **`session_id`, `round`** — joinability. Can always join metrics row back to session log.

Broader rule: **measure things you will be asked about during incident, before incident.**

What you do NOT want to measure at first:

- **Prompt text** — goes in logs, not metrics
- **Internal model state** — you don't have it
- **Success rate** — hard to define, harder to measure honestly
- **Anything derived from multiple rounds** — derive at query time

## 10.8 Worktree Isolation: The Strongest Sandbox

Chapter 5 was about sandboxing: canonical paths, workdir roots, bash allowlists. All assumes agent runs directly in your working tree. Fine for demo, dangerous for tool you hand to other people.

Git worktrees are the defense. A **worktree** is separate checkout of same repository, rooted in different directory, pointed at different branch. From OS perspective it's a full working tree. From git's perspective it shares object database with main checkout, costs almost nothing.

```rust
pub struct WorktreeManager {
    repo_root: PathBuf,
    worktrees_dir: PathBuf,
    index: WorktreeIndex,
    events: EventBus,
    git_available: bool,
}
```

Creating a worktree runs actual `git worktree add`:

```rust
let output = std::process::Command::new("git")
    .args(["worktree", "add", "-b", &branch, path.to_str().unwrap_or("")])
    .current_dir(&self.repo_root)
    .output()
    .map_err(|e| format!("Failed to run git: {e}"))?;
```

Note choice: **shell out to git, don't reimplement it.** Shelling out gives exactly semantics git itself guarantees, without "which version of libgit2 are we on today" drift.

## 10.9 The Event Bus

```rust
pub struct EventBus {
    path: PathBuf,
}

impl EventBus {
    pub fn emit(&self, event: &str, task: Option<serde_json::Value>,
                worktree: Option<serde_json::Value>, error: Option<&str>) {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let payload = serde_json::json!({
            "event": event,
            "ts": ts,
            "task": task.unwrap_or(serde_json::json!({})),
            "worktree": worktree.unwrap_or(serde_json::json!({})),
            "error": error,
        });
        // append to file
    }
}
```

Same JSON Lines shape as metrics. Different purpose: events record **causal transitions**, not numeric samples. When manager creates worktree, emits `worktree.create.before`, runs git, emits `worktree.create.after` or `worktree.create.failed`.

The `.before`/`.after` pairing exists so that if process dies *between* two, you have forensic trail: "we were trying to create worktree `foo` when we died." Without `.before` event, next session would find half-created worktree in git's view and no record how it got there.

This is **transactional event logging**. Log first, recover second.

## 10.10 From Demo to Tool: A Checklist

If you are building your own agent harness and wondering what separates "works on my machine" from "I can hand this to someone else":

1. **Stderr for diagnostics, stdout for output.** Never mix.
2. **Session log file, timestamped, flushed per line, rotated.** Plain text fine as long as markers greppable.
3. **Metrics stream, JSON Lines, one record per round.** Narrow schema. Stable keys.
4. **Event bus for lifecycle transitions**, with before/after pairs.
5. **Workspace isolation** — ideally via git worktrees.
6. **Retention policy on everything that grows**, even trivial one.

Every item costs maybe an afternoon to build. Every agent that ships without them pays for it eventually, usually at worst possible moment. These are not advanced features — they are minimum viable boring.

## 10.11 What We Did Not Cover

Ten chapters cannot cover everything:

- **Streaming responses** — agent treats each LLM response as single blob. Real agents stream tokens.
- **Caching** — prompt caching and tool-result caching shave meaningful cost.
- **Evaluation** — cannot tell whether prompt change made agent better without eval suite.
- **Multi-agent orchestration** — team protocols, hand-off patterns.
- **Human-in-the-loop** — approval flows, diff review.
- **Production deployment** — running as service, authenticating users, isolating tenants.

Each is a rabbit hole. None is prerequisite for understanding core loop.

## 10.12 Where to Go Next

You now have a mental model of a coding agent as a loop over a message list, dispatching tool calls against sandboxed filesystem, with enough plumbing to survive contact with real users. That model generalises: every modern agent — Claude Code, Cursor, Aider, Devin — is some version of same pattern.

The best way to internalise is to build one. Start with `rust_toy_agent` as reference, or start from scratch in whatever language you prefer. First version should do one thing: read a file user names. Add `write_file`. Add `bash`. Add a todo list. Add a retry. Notice which pieces feel hard — those are pieces you half-understood from this book and will truly understand only after implementing them once and debugging session at 2 a.m.

The second-best way is to read source of real agent. Aider and OpenHands are open and worth an afternoon. You will recognise most shapes from this book and find a few new ones.

One last thing. The field is moving fast. What will NOT change is shape of problem: an agent is a loop, a loop needs state, state needs structure, structure needs maintenance, maintenance needs observability. Build on those bones, and build something other people can run.