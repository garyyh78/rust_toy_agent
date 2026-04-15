# Chapter 9: Subagents and Background Work

> "Divide and conquer is not just an algorithmic strategy; it is a way of managing attention." — paraphrased from Dijkstra

A single agent loop has a single mind. It works on one thing at
a time, remembers everything it has seen since the start of the
session, and eventually hits the end of the context window. For
a lot of tasks that is plenty. For harder tasks — large refactors,
cross-cutting bug hunts, exploratory analysis over big codebases
— a single mind is both not enough and too much. Not enough,
because the job is bigger than one context can hold. Too much,
because context full of exploration noise makes the decision-
making worse.

This chapter is about two mechanisms for splitting an agent's
work across more than one mind: **subagents** and **background
tasks**. They solve different problems, they are sometimes
confused, and both of them are in `rust_toy_agent`'s source tree.

## 9.1 Two Problems, Two Mechanisms

Start with the problems they solve.

**Problem 1: the parent's context is getting big, but there's a
clearly-bounded subtask.** The user asked to "find every place we
use the old auth middleware and migrate them." The parent agent
needs to remember the plan, the migration rules, and the list of
files it has already fixed. What it does *not* need to remember
is every tool call and every line of output involved in finding
each use site. But it does need the *answer*: "file X uses the
old middleware on line 42."

This is the subagent case. Spawn a child with a fresh context,
ask it the bounded question, let it burn its own tool calls
exploring, and receive a short summary back.

**Problem 2: the task is going to take a while and the parent
has other things to do.** The user asked to run the full test
suite, which takes two minutes. The parent does not want to sit
at the end of a network connection for two minutes waiting — it
wants to do other tool calls in the meantime and check back on
the test results when they land.

This is the background-task case. Launch the job, get a handle
back immediately, and poll (or be notified) later.

The two mechanisms look superficially similar — both offload
work to some other runner — but they differ in one critical
dimension: **who is doing the thinking**. A subagent has a
whole LLM behind it; it's another agent, making another LLM's
worth of decisions. A background task is dumb — it runs a shell
command and reports the result. Mixing them up leads to
architectures that are either overpowered or underpowered for
their jobs.

## 9.2 Subagents: Fresh Contexts for Bounded Questions

The subagent type in `rust_toy_agent` is small:

```rust
pub struct Subagent {
    client: AnthropicClient,
    workdir: String,
    model: String,
    child_tools: Json,
}
```

Four fields. Note what is *not* here: there is no `Messages`
history, no todo list, no session logger. A subagent is
constructed once per spawn, runs, and is thrown away. Its
context is scoped to exactly one invocation.

The run method is a miniature version of `agent_loop`:

```rust
pub async fn run_subagent(&self, prompt: &str) -> String {
    let mut messages = vec![serde_json::json!({
        "role": "user",
        "content": prompt
    })];
    let system = format!(
        "You are a coding subagent at {}. Complete the given task, \
         then summarize your findings.",
        self.workdir
    );
    for _ in 0..MAX_SUBAGENT_TURNS {
        // call LLM, dispatch tools, append results
        // break when stop_reason != "tool_use"
    }
    Self::extract_summary(&messages)
}
```

Four things to notice:

1. **The prompt is the only input.** A subagent does not inherit
   its parent's conversation, its todo list, or its recent tool
   results. Whatever the parent wants the child to know has to be
   said in the prompt. This discipline is a feature: it forces
   the parent to articulate the task, and the articulation is
   often when you notice the task isn't well-defined.

2. **The system prompt is different from the parent's.** Notice
   "subagent" instead of "agent" and "summarize your findings"
   at the end. The role is different; the expected output shape
   is different. A subagent produces a *summary* for its
   parent — not a finished task, not a plan. Getting the
   expected shape right in the system prompt is how you make
   the parent/child handoff actually work.

3. **The turn count is capped at 30.** Subagents must terminate.
   An agent that spawns a subagent expects the child to return
   in bounded time; a child that loops forever is a worse
   problem than a parent that loops forever, because the parent
   is blocked and the human can't see what's happening. The cap
   is a circuit breaker.

4. **The return value is just the summary string.** The parent
   does not see the child's tool calls, its failed attempts, or
   its exploratory reads. It sees the one thing it asked for: a
   concise answer. This is where subagents earn their keep: the
   child's entire cognitive labour is compressed into maybe 200
   tokens of text, which is what the parent actually needs.

## 9.3 When to Spawn a Subagent

Subagents are not free. A subagent call is a full LLM session
in miniature — multiple API round-trips, real tokens, real
latency. Use one when the savings justify the cost, and not
before. A rough heuristic:

**Spawn a subagent when the task needs to read more than 5–10
files' worth of context to produce an answer that fits in one
paragraph.** The ratio matters. If the child is going to burn
20,000 input tokens to produce 200 output tokens that the parent
will then use, the parent's context is saved 20K - 200 = ~19.8K
tokens for the cost of a second LLM conversation. That is a
great trade.

If the child is going to burn 500 input tokens to produce 500
output tokens, the trade is bad: the parent could have done the
same work in its own context for less total API spend, and the
answer would be fresher (no inter-agent copy).

Some concrete use cases:

* **Code search.** "Find every caller of `parse_config` and
  summarize how they handle errors." The child grep-walks the
  codebase, reads the callers, and returns five sentences.
* **Large-file reading.** "Summarize the schema changes in
  `migrations/`." The child reads thirty SQL files, the parent
  gets the conclusion.
* **Test-runner analysis.** "Run the test suite and tell me
  which tests fail with what errors." The child runs the suite,
  parses the output, and returns a structured summary.

Things that *look* like subagent tasks but aren't:

* **"Make these three edits."** The parent already knows where
  the edits go — there is no context to save. Just do them.
* **"Call the weather API."** A single tool call does not need
  an agent. A plain HTTP tool is fine.
* **"Plan the next few steps."** Planning is the parent's core
  job. Offloading it to a subagent is exactly the kind of move
  that leads to incoherent agents — the parent no longer owns
  its own plan.

## 9.4 The Tool Set Trick

From Chapter 4: the subagent sees a *different tool set* than
the parent. The child gets `bash`, `read_file`, `write_file`,
`edit_file`. It does not get `todo`, `subagent`, or `background_task`.

The reasons are subtle and important:

1. **No todo tool.** A subagent runs for at most 30 turns on a
   bounded task. It does not need planning. Giving it a todo
   tool invites it to spend turns on meta-planning instead of
   doing the work it was asked for.

2. **No recursive subagents.** If the child could spawn further
   subagents, you would get fan-out explosions (a parent spawns
   three children, each spawns three grandchildren, etc.), and
   the token bill would become unpredictable. The one-level
   depth limit keeps the budget knowable.

3. **No background tasks.** A subagent has a tight lifetime —
   at most 30 LLM turns, often just a few seconds of wall
   clock. Launching a background task from inside it would
   create orphaned jobs: the child disappears but the shell
   command keeps running. Keep background tasks at the parent
   level where their lifecycle matches the session.

Every subtraction here corresponds to a failure mode. If you
build your own subagent system, start with the parent's toolset
and subtract tools deliberately, noting which failure mode each
subtraction prevents.

## 9.5 Background Tasks: Fire and Forget

The other mechanism. `background_tasks.rs`:

```rust
pub struct BackgroundManager {
    tasks: Arc<DashMap<String, BackgroundTask>>,
    tx: mpsc::UnboundedSender<Notification>,
    rx: Arc<Mutex<mpsc::UnboundedReceiver<Notification>>>,
}
```

Three things:

* **A concurrent map** of task ID → task state. `DashMap` is a
  sharded hashmap designed for concurrent reads and writes from
  multiple threads without blocking each other. Picking a
  concurrent collection here — instead of `Mutex<HashMap>` —
  means the harness can read the task list while the thread
  pool is writing into it.
* **A channel** for notifications. When a background task
  finishes, the thread running it sends a `Notification` on the
  channel. The main loop drains the channel on each round and
  feeds the notifications back to the agent.
* **A mutex around the receiver.** The sender (`tx`) can be
  cloned freely, but the receiver cannot; it gets wrapped in a
  mutex so the drain can happen from any thread.

Launching a task:

```rust
pub fn run(&self, command: &str, workdir: &Path) -> String {
    let task_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    // ... clone state for the thread ...
    thread::spawn(move || {
        let output_result = build_command(&command_owned, &workdir).output();
        // ... build notification, send on channel ...
    });
    format!("Started background task {task_id}: {command}")
}
```

The call returns *immediately* with a task ID. The agent can
keep working; the shell command runs on an OS thread; when it
finishes, its result appears on the notification channel.

The genius of the design is what happens next. On the main
loop side:

```rust
// Before each LLM call, drain the notification queue
let notifications = bg_manager.drain_notifications();
for n in notifications {
    messages.push(json!({
        "role": "user",
        "content": format!(
            "[background task {}] {} (status: {})",
            n.task_id, n.result, n.status
        )
    }));
}
```

Completed background tasks are **injected into the conversation
as synthetic user messages**. The model sees them on its next
turn as if the human had pasted the result in. No polling tool,
no status-check ritual, no "please check if my task is done" —
the result simply appears when it is ready.

This is a beautiful pattern and it scales. Long-running builds,
test suites, downloads, analysis jobs — anything that can run
without agent supervision — can be launched into the background,
and the agent can pivot to other work while waiting. When the
job finishes, the agent picks it up on its next round.

## 9.6 When to Use Each

A compact decision tree:

* **The task needs an LLM to make decisions?** → Subagent.
* **The task is a single command that will take more than a
  few seconds, and you have other things to do while it runs?**
  → Background task.
* **The task is quick and you will wait for it?** → Just a
  tool call. Neither mechanism is needed.
* **The task is huge and open-ended?** → Neither. Break it
  down by hand or with the human; no amount of
  orchestration fixes an under-specified goal.

The common beginner mistake is reaching for subagents
everywhere. A subagent is appealing because it *feels like
parallelism*, but each subagent is sequential inside (30-turn
loop, one LLM call at a time), and the parent is blocked while
the subagent runs. You do not get wall-clock parallelism from
subagents by default.

You *do* get wall-clock parallelism from background tasks,
because they run on OS threads and the agent carries on with
other work. If your constraint is latency, background tasks
are the right hammer. If your constraint is context bloat,
subagents are.

## 9.7 Concurrency Primitives: What the Harness Uses

The shape of `BackgroundManager` shows three of the
concurrency tools a modern agent harness reaches for:

* **`Arc<T>`** — reference-counted shared ownership. The
  background thread needs the task map; the main thread also
  needs it; both have to hold a pointer that keeps the map
  alive until the last reference drops. `Arc` is the "multiple
  owners, one value" primitive.

* **`Mutex<T>`** — mutually exclusive access. Wrap a value in
  a mutex and any thread can lock it, read or write it, and
  release the lock. Slow when contended, fast when not.

* **`mpsc` channel** — multi-producer, single-consumer queue.
  The background threads push notifications in; the main loop
  drains them out. Channels decouple producers from consumers
  and handle all the synchronisation internally.

The rule of thumb for picking between them:

1. **Need multiple threads to share a single value?** Use
   `Arc<Mutex<T>>`. Clone the Arc to each thread; lock the
   mutex to access.
2. **Need one value that is read concurrently and rarely
   written?** Use `Arc<RwLock<T>>` instead — many readers, one
   writer.
3. **Need a concurrent map specifically?** Use `DashMap`.
   Under the hood it shards the map into multiple buckets
   with finer-grained locks.
4. **Need to pass values between threads?** Use a channel.
   Channels work exceptionally well when the boundary between
   "producer work" and "consumer work" is clean.

`BackgroundManager` uses all four categories at once, which is
roughly what any non-trivial agent harness ends up doing. None
of this is Rust-specific — the same patterns are standard in
Go (channels, sync.Map, sync.Mutex), Python (queue.Queue,
threading.Lock), TypeScript (worker_threads, MessageChannel).
The naming varies; the shapes do not.

## 9.8 The Thread-per-Task Gotcha

`BackgroundManager` spawns an *OS thread* per task:

```rust
thread::spawn(move || {
    let output_result = build_command(&command_owned, &workdir).output();
    // ...
});
```

Not a tokio task. Not a thread pool. A fresh OS thread per call.

This is fine at low concurrency — dozens of background tasks —
and falls over at high concurrency — thousands. A production
agent would use a thread pool (`rayon`, `tokio::task::spawn_blocking`,
or `std::thread::Builder` with a bounded semaphore) to cap the
number of concurrent threads.

Why does `rust_toy_agent` use raw threads anyway? Three reasons:

1. **Simplicity.** `thread::spawn` is one line and has no
   surrounding machinery. A thread pool adds a module worth
   of code.
2. **The workload is self-limiting.** An agent launches at most
   a few background tasks per session, because the human is
   waiting on the session. No real need for a pool.
3. **Shell commands are the workload.** `bash` calls involve
   `fork + exec`, which already costs more than creating a
   thread. Optimizing the thread creation is pointless if the
   command runs for ten seconds.

The lesson: **start simple, and add a pool only when you have
evidence you need one**. "Evidence" here means a production
incident or a benchmark, not a speculative "what if the user
launches 10,000 background tasks." For every real agent, the
answer to that question is "then something else is broken."

## 9.9 Exercises

1. Read `subagent.rs` and count the ways a subagent's context
   is *smaller* than the parent's. (No history, no todo, no
   subagent tool, etc.) For each, describe the failure mode
   it prevents.

2. Suppose you wanted subagents to be able to spawn *more*
   subagents, up to one level deeper. Sketch the changes:
   what tool set? what depth counter? what budget cap?

3. The background-task notification pattern injects results
   as synthetic user messages. What would change if you
   injected them as tool_results instead? Are there any
   subtleties with the tool_use pairing invariant?

4. Write a test for `BackgroundManager` that launches two
   tasks — `echo fast; sleep 0.1` and `sleep 0.5; echo slow`
   — and verifies the fast one's notification arrives first.
   What synchronisation primitives does the test need?

5. Design a variant of the subagent that can be *streamed*:
   instead of returning a single summary at the end, it
   emits incremental results as they become available. How
   does the parent consume them? Does this architecture
   change the decision calculus for "when to spawn"?

In Chapter 10 we step back one last time and talk about
shipping: session logging, metrics, git worktree isolation,
and the operational concerns that turn a demo into a tool
other people can run.
