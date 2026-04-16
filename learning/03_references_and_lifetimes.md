# Chapter 3: The Agent Loop

> **"When in doubt, use brute force."** — *Ken Thompson*

---

## Introduction: The Universal Agent Pattern

Regardless of how sophisticated an agent framework may appear on the surface, every single agent in existence ultimately boils down to a remarkably simple loop structure:

```
1. Send the conversation history to the model
2. Read the model's response carefully
3. If it explicitly requested a tool call, execute it and append the result
4. Otherwise, conclude the interaction
5. Return to step 1 and repeat
```

That is genuinely the **entire job** of the agent harness. The remaining eight chapters of this book (Chapters 2 through 10) are essentially elaborate **refinements and optimizations** of this core loop pattern. In this pivotal chapter, we will pin down the fundamental loop by thoroughly examining `src/agent_loop.rs` — the compact forty-line `agent_loop` function that conceptually holds the entire `rust_toy_agent` system together as a cohesive unit.

Understanding this loop is essential because it is the **common substrate** that every agent system shares, whether built in Rust, Python, or any other language. The specific API calls and error handling vary enormously, but the abstract loop pattern is universal.

---

## 3.1 The Shape and Structure of a Message

Before we dive into the loop itself, we must understand the precise format of the messages that flow through it. Every single message that the agent sends or receives is fundamentally a JSON object with exactly two top-level fields: a `role` that identifies the speaker and `content` that carries the actual payload:

```rust
// A type alias for clarity — this is simply a vector of JSON objects
pub type Messages = Vec<Json>;
```

This representation — a simple vector of JSON blobs rather than a rigidly typed Rust struct — is an **intentionally deliberate architectural choice**. Here is precisely why this matters: the Anthropic Messages API (and OpenAI's equivalent, and every other major LLM API that has emerged over the past two years) has been continually adding new fields to messages at a rapid pace. A fully typed, statically-verified Rust representation would become outdated and require constant maintenance before the code even ships to production. A humble `Vec<Json>` is arguably the "lazy" but absolutely correct answer for long-term maintainability.

### The Three Essential Roles

You will encounter exactly three distinct roles in message history:

| Role | Meaning | Notes |
|------|---------|-------|
| **`"user"`** | Human input OR harness reporting tool results | After the first turn, every `user` message is actually the harness impersonating a human, not an actual person. |
| **`"assistant"`** | What the model produced | Content is an **array of blocks**: text blocks, tool-use blocks, and (on some APIs) thinking blocks. |
| **`"system"`** | System-level instructions | On Anthropic's API, this lives in a separate top-level `system` field rather than as a message. On OpenAI's API, it traditionally appears as the first message with `role: "system"`. Chapter 8 explores system prompts in comprehensive detail. |

### A Concrete Working Example

Here is a concrete real-world example showing exactly two turns of conversation history **after** a single tool call has been executed:

```json
[
  {"role": "user", "content": "Count Python files in src/"},
  {"role": "assistant", "content": [
    {"type": "text", "text": "I'll run find to count the Python files."},
    {"type": "tool_use", "id": "tu_01", "name": "bash",
     "input": {"command": "find src -name '*.py' | wc -l"}}
  ]},
  {"role": "user", "content": [
    {"type": "tool_result", "tool_use_id": "tu_01", "content": "42\n"}
  ]}
]
```

### The Three Critical Invariants

There are **exactly three invariants** embedded in that seemingly simple blob, all of which are actively enforced in code we are about to read:

| # | Invariant | Enforced By | Failure Consequence |
|---|----------|------------|-------------------|
| 1 | Every `tool_use` block has a **unique `id`** | Dispatcher allocates IDs | API rejects with 400 error |
| 2 | Every `tool_use` is followed **immediately** by a `user` message containing a `tool_result` with matching `tool_use_id`** | `validate_tool_pairing()` | API rejects with 400 error |
| 3 | Ordering within the `content` array is strictly preserved: text first, then tool_use, then more text, and repeating | API contract | API rejects with 400 error |

**Violate absolutely ANY of these** and the API will return a 400 HTTP error — and significantly worse, the **entire conversation becomes permanently poisoned** because the model has absolutely no way to recover from an unpaired tool call in its own history. The implications are severe: a single mistake can corrupt an entire session irreversibly.

---

## 3.2 The Agent Loop, Completely Unabridged

Here is the complete `agent_loop` function, reformatted slightly for publication. Read it through once now with full comprehension; we will dissect it **line by line** immediately afterward:

```rust
pub async fn agent_loop(
    client: &AnthropicClient,
    model: &str,
    system: &str,
    tools: &Json,
    messages: &mut Messages,
    workdir: &WorkdirRoot,
    todo: &Mutex<TodoManager>,
    logger: &mut SessionLogger,
) -> (u64, u64, u32) {
    let mut total_input_tokens = 0u64;
    let mut total_output_tokens = 0u64;
    let mut round = 0usize;
    let mut rounds_since_todo = 0usize;

    loop {
        round += 1;
        log_round_start(logger, round, messages.len(), model);

        if let Some(err) = validate_tool_pairing(messages) {
            // History is corrupt — abort immediately
            return (0, 0, 0);
        }
        truncate_messages(messages, 8);

        let Some(mut response) = call_llm(client, model, system,
            messages, tools, logger).await else {
            return (total_input_tokens, total_output_tokens, round as u32);
        };

        let stop_reason = response["stop_reason"].as_str().unwrap_or("").to_string();
        let content = response["content"].take();
        total_input_tokens  += response["usage"]["input_tokens"].as_u64().unwrap_or(0);
        total_output_tokens += response["usage"]["output_tokens"].as_u64().unwrap_or(0);

        messages.push(json!({"role": "assistant", "content": content}));

        if stop_reason != "tool_use" {
            return (total_input_tokens, total_output_tokens, round as u32);
        }

        let (mut results, used_todo) =
            dispatch_tool_calls(&content, workdir, todo, logger);

        rounds_since_todo = if used_todo { 0 } else { rounds_since_todo + 1 };
        maybe_inject_nag(&mut results, rounds_since_todo, logger);

        messages.push(json!({"role": "user", "content": results}));
    }
}
```

### The Complete Picture

That is genuinely **the entire agent loop**. Eight parameters flow in, one loop executes the core logic, and approximately fifty lines of actual code implement the complete behavior. **Absolutely everything else** in the entire `src/` directory is simply something that this function calls OR something that helps it call something else. The elegance and simplicity of this design is worth appreciating.

---

## 3.3 Termination: Precisely When Does the Agent Stop?

Let us examine the **stopping condition** with complete attention:

```rust
if stop_reason != "tool_use" {
    return (total_input_tokens, total_output_tokens, round as u32);
}
```

**The model itself decides autonomously when to stop.** It accomplishes this by producing a response containing absolutely any `stop_reason` value OTHER than `"tool_use"` — most commonly either `"end_turn"` (the model genuinely believes it is finished with the task) or `"max_tokens"` (the response hit the configured output token cap). The harness **absolutely does not second-guess** this decision; if the model says it is done, the harness accepts that judgment immediately and terminates cleanly.

### The Counter-Intuitive Truth

This is the **single most counter-intuitive concept** about tool-use loops that trips up **every single first-time agent builder**. You might reasonably expect a harness to count rounds, monitor for specific text signatures, or look for a magic "I'm finished" tool call. **None of those approaches works well in practice.** Modern tool-use APIs already explicitly distinguish between "this response ends with a tool call and requires another round" versus "this response represents my final answer" — and the absolutely correct engineering choice is to completely trust that signal.

### Safety Limits for Subagents

What about imposing safety limits? Inspect `subagent.rs` to see how this is handled elsewhere:

```rust
const MAX_SUBAGENT_TURNS: u32 = 30;
// ...
for _ in 0..MAX_SUBAGENT_TURNS {
    // Agent execution continues here...
}
```

Subagents receive a **hard cap** on turns. The main agent deliberately does **NOT** have this limit in the current `rust_toy_agent` implementation — in this codebase, the human operator runs `Ctrl+C` if they want to terminate early and bail out. In a genuine production system, you would absolutely want to add a cap here as well, specifically sized for the expected task difficulty: perhaps **30 rounds** for a focused quick job, or as many as **300 rounds** for a long-duration execution. Importantly, this cap would **not** serve as a termination signal in the same sense — it functions as a **circuit breaker**, and when it trips, you should surface that fact **loudly and clearly** rather than silently continuing or aborting obscurely.

---

## 3.4 Step 1: Comprehensive Validation Before Sending

The absolutely **first** thing that occurs inside the loop body is rigorous validation:

```rust
if let Some(err) = validate_tool_pairing(messages) {
    // History is fundamentally corrupted — abort immediately
    return (0, 0, 0);
}
```

The `validate_tool_pairing` function walks through the **entire history** and performs comprehensive checking that every `assistant` message containing a `tool_use` is **immediately followed** by a `user` message containing a matching `tool_result` with the correct `tool_use_id`. If this invariant is violated in ANY way, the function immediately bails out and returns an error.

### Why Validate on EVERY Single Round?

You might wonder why we perform this validation on **every single round** rather than just once. The answer is straightforward: because **you will write bugs**. You will inadvertently push a message in the wrong order, forget to append a required tool result, or accidentally drop a block during the truncation process — and the Anthropic API will punish you severely with a 400 error and an actively unhelpful error message. Validating on the client side **before** the expensive LLM call provides three massive advantages:

| Advantage | Description |
|-----------|-------------|
| **Faster feedback** | The error precisely points at the offending index in YOUR OWN history, not at some obscure line deep in the Python SDK's JSON encoder |
| **No wasted tokens** | A 400 error absolutely counts against your rate-limit budget; failing early and providing a clear error does NOT consume those tokens |
| **No silent poisoning** | If the history becomes broken, **every subsequent round** will also produce a 400 error — and you absolutely need to discover this problem on round one, NOT round fifty |

### Modern Validator Philosophy

Modern agent harnesses contain **dozens** of validators exactly like this one, and they are worth their weight in gold. Think of them as **automated debuggers that run actively on every single request** your system generates.

---

## 3.5 Step 2: Intelligent History Truncation

```rust
truncate_messages(messages, 8);
```

This instructs the system to maintain exactly **eight rounds** — meaning roughly **sixteen messages** (one pair of assistant-plus-user messages per round) — within the live in-context window. All older messages are **permanently dropped** to conserve token space and maintain reasonable context windows.

### The Implementation Contains One Critical Subtility

The actual implementation contains one absolutely critical subtlety that prevents catastrophic bugs:

```rust
let mut cut = messages.len() - target_len;
while cut < messages.len() {
    let prev_is_assistant_tool_use =
        messages[cut - 1]["role"] == "assistant" && /* has tool_use */;
    if prev_is_assistant_tool_use {
        cut += 1;  // Absolutely do NOT split a tool_use/tool_result pair
        continue;
    }
    break;
}
messages.drain(1..cut);
```

**The loop absolutely refuses to cut between a `tool_use` and its corresponding `tool_result`.** If the naive calculated cut point would leave a dangling `tool_use` orphaned at the top of the retained history, it intelligently shifts the cut one message later to preserve the critical invariant.

> **The Golden Rule:** Never break pair invariants simply to save a few tokens. A model whose history starts with an unpaired tool_use will **loop forever** because every single subsequent round it regenerates what it thinks should come next — creating an infinite, unrecoverable cycle.

### The First Message is Sacred

Notice also specifically: `messages.drain(1..cut)` **preserves** `messages[0]`, which is the original user prompt from the human. That anchor message is absolutely **load-bearing** — it is the **only existing record** of what the user actually asked for, and losing it causes the model to completely forget the fundamental goal of the entire task. Anchors like this come up again in Chapter 6 when we discuss more advanced context compaction techniques.

---

## 3.6 Step 3: Making the Actual LLM Call

```rust
let Some(mut response) = call_llm(...).await else {
    return (total_input_tokens, total_output_tokens, round as u32);
};
```

The `call_llm` function performs three absolutely essential operations:

1. **Constructs** the complete request body (the detailed subject of Chapter 7, which covers robust error handling)
2. **POSTs** the request to the LLM API
3. **Logs** both the complete request and the full response for debugging and auditing purposes

If the call fails after absolutely all of its configurable retry attempts have been exhausted, `call_llm` returns `None` — and we **gracefully exit** the entire loop, returning the tokens we have consumed so far so the caller can accurately report them. We absolutely **do not panic**, we absolutely do **NOT retry forever**, and we absolutely do **NOT silently swallow** the error.

### The let-else Pattern

That `let Some(mut response) = call_llm(...).await else { ... }` syntax is specifically **the Rust way** to express "if this optional value is empty, immediately bail from the enclosing function." In equivalent Python, this would be written as:

```python
response = call_llm(...)
if response is None:
    return (total_input_tokens, total_output_tokens, round)
```

You will encounter this pervasive pattern throughout the entire agent framework. The harness is absolutely full of places where "no answer" is a **legitimate outcome** that should propagate all the way up the call stack to the top-level caller.

---

## 3.7 Step 4: Dispatching All Tool Calls

```rust
let (mut results, used_todo) =
    dispatch_tool_calls(&content, workdir, todo, logger);
```

The `dispatch_tool_calls` function performs a comprehensive multi-step operation:

1. It **walks** through the assistant's message content
2. It **finds** absolutely every `tool_use` block in the response
3. It **looks up** each tool by its assigned name in the registry
4. It **runs** each tool with its provided arguments
5. It **builds** a matching `tool_result` block for EACH tool call

We cover the full tool dispatcher implementation itself in Chapter 4. For now, the absolutely critical observation is that **the loop handles multiple tool calls per single round** — not just one.

### The Power of Multiple Tool Calls Per Round

A modern LLM can and frequently WILL return several complete `tool_use` blocks within a **single response**. For example, on round one, the model might produce:

```
text: "Let me explore the repository structure first."
tool_use: bash(ls -la)
tool_use: bash(find . -name '*.toml')
tool_use: read_file(README.md)
```

This is **three separate tool calls** in **one single model turn**. The harness executes all three (in the existing sequential code implementation — see §3.9 for discussion of parallelism), collects the results into a **single unified user message**, and sends them all back together. **Parallelism inside one round represents a completely free performance optimization**: you spent exactly **one single LLM round-trip** and got **three complete tool outputs** in return. Agent frameworks that force strict one-tool-per-round policies leave an enormous amount of potential performance **completely on the table**.

---

## 3.8 Step 5: The Powerful Nag Reminder Pattern

```rust
rounds_since_todo = if used_todo { 0 } else { rounds_since_todo + 1 };
maybe_inject_nag(&mut results, rounds_since_todo, logger);
```

Here is a **pattern worth knowing by name** and understanding deeply. The `maybe_inject_nag` function looks specifically like this:

```rust
fn maybe_inject_nag(results: &mut [Json], rounds_since_todo: usize, ...) {
    if rounds_since_todo >= 3 && !results.is_empty() {
        if let Some(last) = results.last_mut() {
            if let Some(content) = last["content"].as_str() {
                let updated = format!(
                    "{content}\n\n<reminder>Update your todos every time you make progress.</reminder>"
                );
                last["content"] = json!(updated);
            }
        }
    }
}
```

### How the Nag Pattern Works

**Every single round** that the agent goes completely **without touching the todo tool**, the counter increments by one. At **exactly three misses**, the harness appends a brief but clear reminder message to the **next tool result** that the model receives. The model then sees that reminder embedded within a `tool_result` block that it already **completely trusts** — and on the **following round**, it typically updates its todo list as intended.

### The Technical Name

This is formally called a **nag loop** or alternatively **soft steering**. It represents one of the **most powerful techniques** in all of agent engineering, for the **exact same reason** it works so effectively on humans: the reminder arrives **precisely when the target is already paying full attention**, and it cleverly **piggybacks on an already-authoritative channel** that the target treats as inherently trusted.

### Essential Rules for Effective Nag Loops

Here are the essential rules for implementing effective nag loops, from hard-won practical experience:

| Rule | Reasoning |
|------|-----------|
| **Place the nag inside a tool result, NOT a system message** | The model re-reads the system prompt only occasionally (perhaps once per many turns). It reads tool results absolutely every single turn — making this the highest-reach channel. |
| **Tag the nag so the model recognizes it as harness-generated** | The `<reminder>...</reminder>` wrapper is deliberately distinct from any potential real tool output. A model that sees the same nag twice learns to act on it immediately without debate. |
| **Reset the counter rigorously on success** | The moment the model updates its todo list, `rounds_since_todo` returns immediately to zero. Nobody (and no model) appreciates being nagged after they have already completed the requested action. |
| **Absolutely never stack multiple nags simultaneously** | If the agent has forgotten to update the todo list AND has been writing to sensitive system files like `/etc/passwd`, you must prioritize the most urgent concern and send only that single reminder. Three simultaneous reminders are perceived as pure noise and the model begins ignoring ALL of them — including the critical one. |

### The Ultimate Power

**Nag loops are precisely how you transform a distractable model into a focused, effective one** — completely **without retraining** anything whatsoever. We will return to this powerful pattern extensively in Chapter 8 when discussing advanced prompt engineering techniques.

---

## 3.9 What the Loop Deliberately Omits

A small number of important features are **deliberately excluded** from `agent_loop`:

### Feature 1: Parallel Tool Execution
The tool calls within one round run **sequentially**. A tool that reads a file and another that executes a long-running `bash` command do not overlap in time. For a small lightweight coding agent like `rust_toy_agent`, this sequential arrangement is perfectly acceptable. For a production agent with potentially ten-second-long tool calls, you would want to dispatch them **concurrently** using `tokio::join!` or `FuturesUnordered`. Chapter 8 sketches the required changes.

### Feature 2: Streaming Responses
The model produces its output in **one complete shot**. The harness waits for the entire response, then reads the complete JSON body. A streaming client would display tokens as they arrive in real-time — which provides genuinely excellent user experience but adds substantial complexity including handling **partial tool_use blocks**, managing robust **reconnection logic**, and implementing proper **backpressure** mechanics. For a read-only tour agent like this codebase, streaming would not fundamentally change the core architecture.

### Feature 3: Intelligent Compaction
When the conversation history grows excessively long, this loop performs simple **truncation** to a fixed window. It does absolutely **NOT** *summarize* the dropped turns in any way. A sophisticated production agent might instead execute a compaction pass — using a cheaper model to actively summarize the older turns into one or two synthetic messages — and prepend that compact summary to the preserved history. Chapter 6 covers compaction in comprehensive detail.

### Feature 4: Dynamic Tool Loading
The tools vector is currently **passed in once** and **never changes** throughout the entire session. An advanced harness might absolutely swap tools based on the emerging task — for example, **hiding dangerous tools** until the model has proven itself trustworthy, or **revealing project-specific tools** once the workdir is successfully detected and identified. None of these advanced features matter for our present tour of the codebase, but they represent natural extension points.

### The Engineering Philosophy

**Every one** of these features represents a natural and desirable extension point, but **each one** introduces **dozens of additional lines** of code AND **an entirely new category** of potential failure modes. The art lies absolutely in knowing when you genuinely need each feature — and `rust_toy_agent`, for all of its intentional simplicity, is genuinely capable of executing **real, meaningful tasks** without any of them.

---

## Chapter 3 Summary and Transition

In this chapter, we have accomplished several critical objectives:

1. **Established the universal loop pattern** that underlies absolutely every agent system — from the simplest toy implementation to the most sophisticated production deployment.

2. **Understood the message format** in comprehensive detail — including the three critical roles (`user`, `assistant`, `system`) and the three invariants that must be vigorously enforced.

3. **Examined termination conditions** — appreciating why the model having complete control over stopping is actually correct, and why the counter-intuitive aspect trips up so many first-time builders.

4. **Traced the complete flow** through the loop: validation, truncation, LLM call, tool dispatching, and nag injection — understanding precisely how each step enables the next.

5. **Appreciated the nag pattern** as one of the most powerful techniques in agent engineering — a reminder that piggybacks on an already-trusted channel.

6. **Understood what is deliberately omitted** — and why those omissions represent wise engineering choices rather than bugs.

In the **next chapter**, we will zoom deeply into the `dispatch_tool_calls` function and address the other genuinely hard question in agent engineering: **exactly what tools should we deliberately expose, and how should each tool behave when the model uses it incorrectly?**

---

**Next:** Chapter 4 — Tool Design and Sandboxing