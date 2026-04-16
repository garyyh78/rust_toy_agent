# Chapter 10: Observability and Shipping

> "In theory there is no difference between theory and practice.
> In practice there is." — attributed to Yogi Berra, and to anyone
> who has ever tried to debug an agent at 3 a.m.

Nine chapters in, `rust_toy_agent` is a working coding agent. It
holds a conversation, dispatches tools, recovers from flaky
networks, plans with a todo list, spawns subagents, and runs
shell commands in the background. It is a demo that runs on your
laptop. What it is *not* is a tool that other people can safely
point at their repositories.

The gap between those two things is the subject of this final
chapter. It is not a gap in cleverness — the clever parts are
already done. It is a gap in *operability*: the unglamorous
plumbing that turns a thing that works on your machine into a
thing you can hand to someone else, debug from a bug report,
diff against yesterday's behaviour, and unwind cleanly when it
makes a mess. Four pieces of plumbing:

1. **Session logs** — a record of what the agent said and did,
   written as the session runs.
2. **Metrics** — structured numbers per round, suitable for
   aggregation and dashboards.
3. **Worktree isolation** — a sandbox stronger than "trust the
   agent not to step on your working tree."
4. **Event buses** — a causal record of lifecycle transitions
   that a human can replay when something goes wrong.

None of these is hard to build. All of them are easy to
under-build. And the difference between an agent you can ship
and an agent you cannot is usually found here, not in the
prompt or the model.

## 10.1 Two Readers, Two Formats

Before looking at code, notice something about observability:
**you are writing for two different readers at once**, and they
want different things.

Reader one is a human. A human wants a running transcript in
chronological order, human-readable, colourised when it helps,
skimmable when the session is long. The human reader is
usually either watching a live run in a terminal or reading a
log file after the fact to understand why a session failed.
Prose is fine. Timestamps are nice. Line wrapping matters.

Reader two is a machine — a dashboard, a pipeline, a script
that rolls up numbers across a thousand sessions. The machine
does not care about formatting; it wants structured records
with stable keys, one per line, parseable without ambiguity.
Prose is actively hostile to the machine. Timestamps must be
machine-readable. Line wrapping is a bug.

Good observability ships *both*. Bad observability tries to
ship one format and hopes the other reader will cope. The
common failure is a beautiful human log that no pipeline can
parse, or a JSON firehose that no human can read under
pressure. `rust_toy_agent` keeps the two separate, on purpose,
and we will see why.

## 10.2 The Session Logger

`logger.rs` is the human-facing side. The type is small:

```rust
pub struct SessionLogger {
    file: Option<File>,
}
```

One field. An optional file handle. That is all the state a
session logger needs, because its job is to fan out calls to
two sinks — stderr and a file — and let the caller pretend
there is only one sink.

The methods come in two flavours. Free functions for stderr-
only output:

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

And methods on `SessionLogger` that do the same stderr write
*and* also append a timestamped line to the log file:

```rust
pub fn log_section(&mut self, title: &str) {
    log_section(title);
    self.write_file(&format!("[{}] === {} ===", Self::timestamp(), title));
}
```

Notice the pattern. The method delegates to the free function
for the stderr half, then writes to the file. The stderr output
is identical whether you are logging to a file or not, which
means live terminals look the same in both modes. The file
output adds a timestamp and a little framing, because the file
will be read later, possibly months later, and the human
reader's first question will be "when did this happen."

Three details in this file are worth dwelling on.

**Stderr, not stdout.** Every diagnostic write uses `eprintln!`,
never `println!`. This is a discipline, not an accident. The
agent's REPL puts the model's prose on stdout; everything else
goes on stderr. That separation means you can pipe the agent to
`jq` or `tee` without the logs getting in the way, and it means
a user who redirects stdout to a file gets just the agent's
output, not a mix of output and diagnostics. **Pick a side for
logs and never cross.** If you are writing a CLI tool that
produces structured output and also wants to log, stdout is
data and stderr is conversation. This convention is older than
any of us and there is no good reason to fight it.

**Colours on stderr, plain text in the file.** The stderr write
uses Unicode block characters and (in a richer build) ANSI
escapes for colour. The file write uses plain ASCII. A terminal
renders the stderr output beautifully; a text editor renders
the file output readably. If you shipped colours to the file
you would see `\x1b[31m` sequences everywhere and `grep` would
be unhappy. **Strip presentation on the way to durable storage.**

**Timestamps on the file, not on stderr.** Stderr is "now," so
a timestamp on every line is clutter. A file is "then," so
without a timestamp every line is unmoored. The shape of the
output follows the shape of the reader's question.

And one tiny helper that is easy to overlook:

```rust
fn write_file(&mut self, line: &str) {
    if let Some(ref mut f) = self.file {
        if let Err(e) = writeln!(f, "{line}") {
            tracing::error!(error = %e, "log file write failed");
        }
        if let Err(e) = f.flush() {
            tracing::error!(error = %e, "log file flush failed");
        }
    }
}
```

The `f.flush()` after every line is deliberate. Buffered writes
are faster, but a session that crashes mid-round leaves an
empty log file if the tail of the session is stuck in an OS
buffer. Flushing every line costs a little throughput and gains
you the one thing you need from a log file: **the last line
before the crash**. For an observability tool, that trade is
never close.

## 10.3 Logging User and Agent Turns

Two methods sit slightly apart from the rest:

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

These are file-only. They do not echo to stderr, because the
terminal already showed the user their own input (they just
typed it) and the agent's response (it was just printed). The
file is the only place that needs both sides.

The `USER >` and `AGENT <` arrows are more than decoration.
They give you the one affordance that matters when you are
debugging from a log: you can grep for every user message with
`grep 'USER >'` and every agent response with `grep 'AGENT <'`.
Without arrows, you would be writing a parser.

**A log format is a contract with future-you.** Pick marker
strings that are unambiguous, short, and easy to grep, and
then do not change them, because the scripts future-you is
going to write will assume the markers.

## 10.4 API Request and Response Logging

Two more methods handle the noisiest case:

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

Every line of the pretty-printed JSON gets its own timestamp.
That looks wasteful — why not one timestamp at the top? —
and for a while you think it is, and then one day you are
debugging a 4 a.m. incident where the agent hangs mid-request
and you discover that the request body was 40 KB and only half
of it was written to the file before the process died. The
line-level timestamps tell you exactly how far the write got.

Logging full API bodies is expensive. Do it anyway, at least
in development and during incidents. The number of bugs that
are obvious once you can see the request body is astonishingly
large. The number of bugs that are obvious *without* the
request body is correspondingly small. If you are worried
about disk space, rotate the logs (see the next section), not
the content.

## 10.5 Log Rotation: The Simplest Pattern That Works

Log files grow. On a laptop, 20 sessions of full API logging
might be a gigabyte. You need a policy. `logger.rs` ships the
simplest one that works:

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

Keep the 20 most recent session files, delete the rest. That is
it. No size-based rotation. No compression. No database. Twenty
is an arbitrary number picked because 20 sessions is "enough to
find yesterday's problem and a couple days before it" without
being "so many that the logs dir is huge."

The lesson here is not about the exact policy. It is about
**building the retention behaviour at all**. An agent with a
growing log directory and no pruning is a pager alert waiting
to happen — six weeks in, someone's disk fills up, the agent
starts failing for reasons unrelated to the agent, and you
spend an afternoon tracing the problem to a line you never
thought about. Ten lines of pruning at the top of the file
save an afternoon of confused debugging later.

One subtlety: the pruning runs when a new `SessionLogger` is
created. That is a lazy, opportunistic schedule — the old logs
don't get pruned until the next session starts — but for this
workload it is perfect. Sessions are infrequent enough that
pruning at session start is free, and you avoid a background
thread whose only job is cleanup.

**Do the simple thing. Revisit when it breaks.**

## 10.6 Metrics: The Machine-Facing Side

`metrics.rs` is small enough to print in full:

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

Twenty-six lines and a format. This is what a machine-facing
telemetry layer looks like before you have scale problems to
solve. Four things about the shape are worth copying:

**One record per round, append-only.** Every round, one line.
No updates, no batching, no buffering. The format is JSON
Lines — newline-delimited JSON — which is the de facto
standard for streaming structured logs because it survives
truncation (the last line might be broken, but every complete
line before it is still valid) and you can process it with
`jq -c` or pipe it into any log aggregator.

**The schema is narrow and fixed.** Eight fields. Each one has
a clear, non-overlapping purpose: `input_tokens` and
`output_tokens` for cost, `wall_ms` for latency, `tool_calls`
and `retries` for behaviour, `session_id` and `round` for
joinability, `timestamp` and `host` for context. Notice what
is *not* here: no model name, no prompt text, no tool outputs.
Metrics are for aggregation; the context that made a round
interesting belongs in the session log. Conflating the two
gives you a log file no pipeline can parse and a metrics stream
no human can read.

**`&'static str` for the host.** Reading the Rust carefully:
`host: &'static str` means the host string is a compile-time
constant, not a heap allocation. That's deliberate — the host
is a build-time property ("linux-x86_64", "darwin-arm64"), and
allocating a new `String` for every round of every session
would be wasteful. When a field genuinely cannot change at
runtime, `&'static str` is the honest type.

**The emit function is transactional-enough.** Open, append,
close. No persistent handle. No "oops we crashed halfway
through writing a record," because JSON Lines records are
whole or absent. If the process dies after `writeln!` but
before `flush()`, the OS will still flush the buffer to disk
when the file descriptor is closed (ungracefully or otherwise)
in the vast majority of cases. For metrics, "vast majority"
is the right target — you accept a small chance of losing the
very last round rather than paying the cost of fsync on every
write.

## 10.7 What Metrics to Collect

The schema above is small, but not arbitrary. Every field
answers a real question someone will ask about the agent:

* **`input_tokens`, `output_tokens`** — "what is this costing
  us?" The single most common question a human operator asks
  about a running agent. Without these, you are flying blind
  on cost.
* **`wall_ms`** — "is the agent getting slower?" Regressions
  in latency are the number-one complaint from users, and
  they are almost always invisible in the logs (every line
  looks fine, the rounds just add up).
* **`tool_calls`** — "is the agent working or thinking?" A
  round with zero tool calls is either the agent finishing
  (good) or the agent chatting (bad). A round with 15 tool
  calls is either the agent making progress (good) or the
  agent thrashing (bad). Either way, the distribution shifts
  when something is wrong.
* **`retries`** — "is the network healthy?" If retry counts
  spike across sessions, something upstream is degraded —
  long before any single session looks broken.
* **`session_id`, `round`** — joinability. You can always
  join a metrics row back to a session log if you have these
  two fields and the log file names them somewhere.

The broader rule: **measure the things you will be asked
about during an incident, before the incident**. Cost,
latency, throughput, error rate. If someone pages you at 3
a.m. saying "the agent is broken," you want to be able to
answer "for whom, how badly, since when" without reading
logs. The metrics stream is what makes that answer fast.

What you do *not* want to measure, at least at first:

* **Prompt text** — goes in logs, not metrics.
* **Internal model state** — you don't have it, don't pretend.
* **Success rate** — hard to define, harder to measure
  honestly. Wait until you know what "success" means in your
  domain.
* **Anything derived from multiple rounds** — derive those at
  query time from the raw per-round rows. If you derive at
  write time, you lock in a shape you will regret.

## 10.8 Worktree Isolation: The Strongest Sandbox

Chapter 5 was about sandboxing: canonical paths, workdir
roots, bash allowlists. All of that assumes the agent runs
directly in your working tree. That is fine for a demo and
dangerous for a tool you hand to other people. The agent is
one `rm -rf` or one misrouted `git checkout` away from
destroying hours of your uncommitted work, and "the agent is
usually careful" is not a safety argument.

Git worktrees are the defence. A **worktree** is a separate
checkout of the same repository, rooted in a different
directory, typically pointed at a different branch. From the
operating system's point of view it is a full working tree.
From git's point of view it shares the object database with
the main checkout, so it costs almost nothing and branches
created in one are visible in the other. The agent runs
inside the worktree; your main checkout is untouched no
matter what happens.

`rust_toy_agent`'s `WorktreeManager` wraps this pattern:

```rust
pub struct WorktreeManager {
    repo_root: PathBuf,
    worktrees_dir: PathBuf,
    index: WorktreeIndex,
    events: EventBus,
    git_available: bool,
}
```

Five fields. The first two locate the worktree tree on disk
(repo root and the `.worktrees/` sibling directory that holds
them). The next two keep durable state: an index of what
worktrees exist and an event bus recording what has happened
to them. The last is a boolean "are we even in a git repo"
that degrades the whole feature gracefully when you run the
agent outside one.

Creating a worktree runs an actual `git worktree add`:

```rust
let output = std::process::Command::new("git")
    .args([
        "worktree", "add", "-b", &branch,
        path.to_str().unwrap_or(""),
    ])
    .current_dir(&self.repo_root)
    .output()
    .map_err(|e| format!("Failed to run git: {e}"))?;
```

Note the choice: **shell out to `git`, don't reimplement it**.
The `git2` crate could create worktrees in-process, but shelling
out to `git` gives you exactly the semantics git itself
guarantees, with none of the "which version of libgit2 are we
on today" drift that bites every long-running project that
tries to reimplement git internals. For operations where
correctness is paramount, let git be the source of truth.

The `binding` module ties worktrees to task IDs so you can ask
"which worktree belongs to task 42" and get an answer without
scanning filesystem state. The `index` module persists the
worktree table to `.worktrees/index.json`, because a fresh
agent process restarting tomorrow needs to rediscover which
worktrees exist without crawling the filesystem. Every piece
of long-lived state the agent manages lives in a deliberate
place on disk, not in RAM.

## 10.9 The Event Bus

The last piece is the event bus. `events.rs`:

```rust
pub struct EventBus {
    path: PathBuf,
}

impl EventBus {
    pub fn emit(
        &self,
        event: &str,
        task: Option<serde_json::Value>,
        worktree: Option<serde_json::Value>,
        error: Option<&str>,
    ) {
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

Same JSON Lines shape as metrics. Same append-only discipline.
Different purpose: events record **causal transitions**, not
numeric samples. When the manager creates a worktree, it emits
a `worktree.create.before` event, then runs `git worktree add`,
then emits either `worktree.create.after` (on success) or
`worktree.create.failed` (on failure, with the error string).
Three events for one operation, and they let a human replay
what happened in order.

The `.before`/`.after` pairing is not decorative. It exists
so that if the process dies *between* the two, you have a
forensic trail: "we were trying to create worktree `foo` when
we died." Without the `.before` event, the next session would
find a half-created worktree in git's view of the world and
no record of how it got there. The pair is a crash-consistent
record of intent.

This is the pattern **transactional event logging**, and it
turns up in every system that has to reason about partial
failures: databases, distributed systems, build tools. You
emit an event when you intend to do something, emit another
when it finishes, and the absence of the finishing event
tells you exactly where a run died. Pair this with an idempotent
retry on startup ("if a `.before` exists without an `.after`,
either roll forward or roll back") and you have the skeleton
of a real recovery protocol.

For now, `rust_toy_agent` does not act on dangling `.before`
events. The information is there; the recovery logic is not.
That is fine — the information being there is a prerequisite
for the recovery logic ever existing, and you cannot write the
recovery logic without the data. **Log first, recover second,
and do not skip the first step because the second step is not
built yet.**

## 10.10 From Demo to Tool: A Checklist

If you are building your own agent harness and wondering what
separates "works on my machine" from "I can hand this to
someone else," the list is shorter than you might expect.

1. **Stderr for diagnostics, stdout for output.** Never mix.
2. **A session log file, timestamped, flushed per line,
   rotated at some bound.** No structured format required —
   plain text is fine as long as the markers are greppable.
3. **A metrics stream, JSON Lines, one record per round,
   append-only.** Narrow schema. Stable keys. Include at
   least tokens, latency, and retry counts.
4. **An event bus for lifecycle transitions**, with
   before/after pairs for anything that can fail halfway.
5. **Workspace isolation**, ideally via git worktrees, so
   that a broken session cannot corrupt your main checkout.
6. **A retention policy on everything that grows**, even a
   trivial one, committed the day you write the logs.

You will notice every item on this list costs maybe an
afternoon to build. You will also notice that every agent
that ships without them pays for it eventually, usually at
the worst possible moment. These are not advanced features;
they are the minimum viable boring.

## 10.11 What We Did Not Cover

Ten chapters of a single codebase cannot cover everything, and
there are real topics we touched only glancingly or skipped
outright. A partial list, so you know what is still out there:

* **Streaming responses.** The agent in this book treats each
  LLM response as a single blob. Real agents stream tokens as
  they arrive, both for perceived latency and because some
  workloads need to react mid-generation.
* **Caching.** Prompt caching (provider-side) and tool-result
  caching (client-side) both shave meaningful cost from any
  agent that re-reads the same files across turns.
* **Evaluation.** You cannot tell whether a change to a prompt
  or a tool made the agent better or worse without an eval
  suite. Building one is its own discipline.
* **Multi-agent orchestration beyond one-level subagents.**
  Team protocols, hand-off patterns, shared scratchpads —
  `rust_toy_agent` has a little of this in `agent_teams.rs`
  but not enough to build an opinion on.
* **Human-in-the-loop.** Approval flows, diff review, "pause
  before this dangerous operation" — important for any agent
  that touches shared systems.
* **Production deployment.** Running the agent as a service,
  authenticating users, isolating tenants, rate-limiting at
  the API edge. Every one of these is a chapter-length
  problem and none of them is Rust-specific.

Each of these is a rabbit hole. None of them is a prerequisite
for understanding the core loop, which is why we spent ten
chapters on the loop instead.

## 10.12 Where to Go Next

If this book worked, you now have a mental model of a coding
agent as a loop over a message list, dispatching tool calls
against a sandboxed filesystem, with enough plumbing around
it to survive contact with real users. That model generalises:
every modern agent — Claude Code, Cursor, Aider, Devin — is
some version of the same pattern, with different tools and
different prompts and different UIs but the same bones.

The best way to internalise the model is to build one. Start
with `rust_toy_agent` as a reference, or start from scratch in
whatever language you prefer. The first version should do one
thing: read a file the user names. Add `write_file`. Add
`bash`. Add a todo list. Add a retry. Notice which pieces feel
hard — those are usually the pieces you half-understood from
this book and will truly understand only after you have
implemented them once and debugged a session at 2 a.m.

The second-best way is to read the source of a real agent.
Claude Code is closed, but Aider and OpenHands are open, and
both are worth an afternoon. You will recognise most of the
shapes from this book and you will find a few new ones — each
project makes slightly different trade-offs, and seeing a
second set of choices is how you learn to make your own.

One last thing. The field is moving fast, and a lot of what
is in this book will look quaint in a few years. Models will
get better. Tool protocols will standardise. Context windows
will grow until the chapter on context management reads like
a historical document. What will *not* change is the shape of
the problem: an agent is a loop, a loop needs state, state
needs structure, structure needs maintenance, and
maintenance needs observability. Whatever the next generation
of harnesses looks like, those are the bones. Build on them,
and build something other people can run.

## 10.13 Exercises

1. Add a `log_tool_call(&mut self, name: &str, input: &Value)`
   method to `SessionLogger`. Write to file only, with the
   same timestamp framing as `log_api_request`. What arrow
   marker would you pick, and why?

2. Extend `RoundMetrics` with a `model` field. It cannot be
   `&'static str` anymore — why? What is the right Rust type,
   and what does that change cost you at the call site?

3. Write a one-page `scripts/metrics_summary.sh` that reads a
   day's worth of JSON Lines from `metrics.jsonl` and prints,
   per session: total tokens, total wall time, retry count.
   Use `jq`. Do not parse the JSON by hand.

4. Implement a recovery routine for the worktree event bus:
   on startup, read the last N events, and for every
   `worktree.create.before` without a matching `.after` or
   `.failed`, either roll the worktree forward (finish the
   `git worktree add`) or back (delete the half-state). Which
   is the safer default, and why?

5. Design, in prose, how you would add a `--record` and
   `--replay` flag to the agent: `--record` writes every API
   response to a file, and `--replay` reads them back instead
   of calling the API. What changes in the loop? What
   changes in the session log? What breaks if the prompt
   changes between record and replay?

6. The very last exercise: pick one agent you use regularly —
   Claude Code, Cursor, Aider, anything — and spend an hour
   trying to find its equivalents of the four pieces of
   plumbing in this chapter. Where are the session logs?
   Where are the metrics? Is there a worktree abstraction?
   How does it log lifecycle events? For each answer, ask
   whether you would do it the same way. This is how you
   develop taste.

---

That is the end of the book. You read a ten-thousand-line
agent from top to bottom, and you now know how every piece
fits together, where the hard parts are, and what a
shippable version looks like. Go build something.
