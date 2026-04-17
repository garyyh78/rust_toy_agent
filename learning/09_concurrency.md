# Chapter 9: Subagents and Background Work

> "Divide and conquer is not just an algorithmic strategy; it is a way of managing attention." — *Dijkstra*

## Introduction

A single agent loop has a single mind. It works on one thing at a time, remembers everything since session start, and eventually hits context window limit. For many tasks that's plenty. For harder tasks — large refactors, bug hunts, exploratory analysis — a single mind is both not enough and too much.

This chapter covers two mechanisms for splitting work across more than one mind: **subagents** and **background tasks**.

## 9.1 Two Problems, Two Mechanisms

**Problem 1:** Parent's context is getting big, but there's a clearly-bounded subtask. User asked "find every place we use old auth middleware and migrate them." Parent needs to remember plan, migration rules, list of files fixed. What it does NOT need is every tool call and line of output. But it DOES need the answer: "file X uses old middleware on line 42."

This is the **subagent case**: spawn child with fresh context, ask bounded question, receive short summary back.

**Problem 2:** Task is going to take a while and parent has other things to do. User asked to run full test suite, takes two minutes. Parent doesn't want to wait — wants to do other tool calls and check back on results later.

This is the **background-task case**: launch job, get handle back immediately, poll (or be notified) later.

Difference: **who is doing the thinking.** Subagent has whole LLM behind it — another agent making decisions. Background task is dumb — runs shell command and reports result.

## 9.2 Subagents: Fresh Contexts for Bounded Questions

```rust
pub struct Subagent {
    client: AnthropicClient,
    workdir: String,
    model: String,
    child_tools: Json,
}
```

Note what's NOT here: no `Messages` history, no todo list, no session logger. A subagent is constructed once per spawn, runs, and is thrown away.

The run method:

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

Four observations:

1. **Prompt is the only input** — subagent does not inherit parent's conversation, todo list, or recent tool results. Whatever parent wants child to know has to be said in prompt.

2. **System prompt is different** — notice "subagent" instead of "agent" and "summarize your findings" at end. Subagent produces a *summary* for parent — not finished task, not plan.

3. **Turn count capped at 30** — subagents must terminate. Child that loops forever is worse problem than parent looping forever.

4. **Return value is just the summary string** — parent does not see child's tool calls, failed attempts, exploratory reads. It sees the one thing it asked for: concise answer.

## 9.3 When to Spawn a Subagent

Subagents are not free. A subagent call is a full LLM session in miniature. Use one when savings justify cost.

**Spawn when task needs to read more than 5–10 files' worth of context to produce an answer that fits in one paragraph.** Ratio matters. If child burns 20,000 input tokens to produce 200 output tokens, parent's context is saved ~19.8K tokens for cost of second LLM conversation. That's a great trade.

If child burns 500 input to produce 500 output, trade is bad. Parent could do same work in own context for less API spend.

Concrete use cases:

- **Code search** — "Find every caller of `parse_config` and summarize how they handle errors"
- **Large-file reading** — "Summarize schema changes in `migrations/`"
- **Test-runner analysis** — "Run test suite and tell me which tests fail"

Things that look like subagent tasks but aren't:

- **"Make these three edits"** — parent already knows where edits go
- **"Call the weather API"** — single tool call doesn't need an agent
- **"Plan the next few steps"** — planning is parent's core job

## 9.4 The Tool Set Trick

From Chapter 4: subagent sees a *different tool set* than parent. Child gets `bash`, `read_file`, `write_file`, `edit_file`. Does not get `todo`, `subagent`, or `background_task`.

Reasons:

1. **No todo tool** — subagent runs at most 30 turns on bounded task. Doesn't need planning.

2. **No recursive subagents** — if child could spawn more, fan-out explosions occur. One-level depth keeps budget knowable.

3. **No background tasks** — subagent has tight lifetime. Launching background task creates orphaned jobs. Keep at parent level.

## 9.5 Background Tasks: Fire and Forget

```rust
pub struct BackgroundManager {
    tasks: Arc<DashMap<String, BackgroundTask>>,
    tx: mpsc::UnboundedSender<Notification>,
    rx: Arc<Mutex<mpsc::UnboundedReceiver<Notification>>>,
}
```

Three things:

- **Concurrent map** — `DashMap` is a sharded hashmap for concurrent reads and writes without blocking
- **Channel for notifications** — when task finishes, thread sends `Notification` on channel. Main loop drains channel on each round.
- **Mutex around receiver** — sender can be cloned freely, receiver cannot

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

Call returns *immediately* with task ID. Agent can keep working; shell command runs on OS thread; when finishes, result appears on notification channel.

What happens next:

```rust
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

Completed background tasks are **injected into conversation as synthetic user messages**. Model sees them on next turn as if human pasted result in.

## 9.6 When to Use Each

- **Task needs an LLM to make decisions?** → Subagent
- **Task is a single command that will take more than a few seconds, and you have other things to do while it runs?** → Background task
- **Task is quick and you will wait for it?** → Just a tool call
- **Task is huge and open-ended?** → Neither. Break down by hand or with human.

Common beginner mistake: reaching for subagents everywhere. Each subagent is sequential inside (30-turn loop), parent is blocked while subagent runs. You do NOT get wall-clock parallelism from subagents by default.

You DO get wall-clock parallelism from background tasks, because they run on OS threads and agent carries on with other work.

## 9.7 Concurrency Primitives

The shape of `BackgroundManager` shows three concurrency tools:

- **`Arc<T>`** — reference-counted shared ownership. Multiple owners, one value.

- **`Mutex<T>`** — mutually exclusive access.

- **`mpsc` channel** — multi-producer, single-consumer queue. Decouples producers from consumers.

Rule of thumb:

1. **Need multiple threads to share a single value?** Use `Arc<Mutex<T>>`
2. **Need one value read concurrently, rarely written?** Use `Arc<RwLock<T>>` — many readers, one writer
3. **Need a concurrent map specifically?** Use `DashMap`
4. **Need to pass values between threads?** Use a channel

## 9.8 The Thread-per-Task Gotcha

```rust
thread::spawn(move || {
    let output_result = build_command(&command_owned, &workdir).output();
    // ...
});
```

Not a tokio task. Not a thread pool. A fresh OS thread per call.

This is fine at low concurrency — dozens of background tasks — and falls apart at high concurrency — thousands. Production agent would use a thread pool to cap concurrent threads.

Why does `rust_toy_agent` use raw threads anyway?

1. **Simplicity** — `thread::spawn` is one line
2. **Workload is self-limiting** — agent launches at most a few background tasks per session
3. **Shell commands are the workload** — `fork + exec` already costs more than creating a thread

**Start simple, add pool only when you have evidence you need one.**

---

**Next:** Chapter 10 — Observability and Shipping