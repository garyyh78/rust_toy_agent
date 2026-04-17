# Chapter 3: The Agent Loop

> "When in doubt, use brute force." — *Ken Thompson*

## Introduction

Every agent boils down to this loop:

```
1. Send the conversation history to the model
2. Read the model's response
3. If it requested a tool call, execute it and append the result
4. Otherwise, conclude the interaction
5. Return to step 1
```

The remaining chapters are refinements of this core loop. We examine `src/agent_loop.rs` — the function that holds the entire system together.

## 3.1 The Shape and Structure of a Message

Every message is a JSON object with a `role` and `content`:

```rust
pub type Messages = Vec<Json>;
```

This representation is deliberate. The Anthropic Messages API (and OpenAI's) continually adds new fields. A rigidly typed Rust representation would require constant maintenance. A humble `Vec<Json>` is correct for long-term maintainability.

### The Three Essential Roles

| Role | Meaning |
|------|---------|
| `"user"` | Human input OR harness reporting tool results |
| `"assistant"` | What the model produced — content is an array of blocks |
| `"system"` | System-level instructions |

A concrete example:

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

1. Every `tool_use` block has a **unique `id`**
2. Every `tool_use` is followed **immediately** by a `user` message with a `tool_result` with matching `tool_use_id`
3. Ordering within the `content` array is strictly preserved

Violate any of these and the API returns a 400 error and the conversation becomes permanently poisoned.

## 3.2 The Agent Loop

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

Eight parameters flow in, one loop, ~50 lines of code. Everything else is what this function calls.

## 3.3 Termination

```rust
if stop_reason != "tool_use" {
    return (total_input_tokens, total_output_tokens, round as u32);
}
```

The model decides when to stop. It produces a response with any `stop_reason` value other than `"tool_use"` — commonly `"end_turn"` or `"max_tokens"`. The harness does not second-guess this.

Subagents receive a hard cap on turns:

```rust
const MAX_SUBAGENT_TURNS: u32 = 30;
```

## 3.4 Step 1: Comprehensive Validation

```rust
if let Some(err) = validate_tool_pairing(messages) {
    return (0, 0, 0);
}
```

This validates on every single round because you will write bugs — push messages in wrong order, forget to append a required tool result. Validating before the LLM call provides faster feedback, no wasted tokens, and prevents silent poisoning.

## 3.5 Step 2: Intelligent History Truncation

```rust
truncate_messages(messages, 8);
```

This maintains exactly eight rounds — 16 messages — within the live window. The implementation refuses to cut between a `tool_use` and its `tool_result`:

```rust
let mut cut = messages.len() - target_len;
while cut < messages.len() {
    let prev_is_assistant_tool_use =
        messages[cut - 1]["role"] == "assistant" && /* has tool_use */;
    if prev_is_assistant_tool_use {
        cut += 1;
        continue;
    }
    break;
}
messages.drain(1..cut);
```

**The Golden Rule:** Never break pair invariants. A model whose history starts with an unpaired tool_use will loop forever.

`messages.drain(1..cut)` preserves `messages[0]` — the original user prompt. That anchor is load-bearing.

## 3.6 Step 3: Making the Actual LLM Call

```rust
let Some(mut response) = call_llm(...).await else {
    return (total_input_tokens, total_output_tokens, round as u32);
};
```

`call_llm` constructs the request body, POSTs to the API, and logs both request and response. If all retry attempts are exhausted, it returns `None` and we gracefully exit.

## 3.7 Step 4: Dispatching All Tool Calls

```rust
let (mut results, used_todo) =
    dispatch_tool_calls(&content, workdir, todo, logger);
```

The dispatcher walks through the assistant's message content, finds every `tool_use` block, looks up each tool in the registry, runs each tool, and builds a matching `tool_result` for each.

The loop handles **multiple tool calls per single round**. A model can return several `tool_use` blocks in one response. The harness executes all three, collects results into a single user message. Parallelism inside one round is a free performance optimization.

## 3.8 Step 5: The Nag Reminder Pattern

```rust
rounds_since_todo = if used_todo { 0 } else { rounds_since_todo + 1 };
maybe_inject_nag(&mut results, rounds_since_todo, logger);
```

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

Every round without touching the todo tool increments the counter. At exactly three misses, the harness appends a reminder to the next tool result. This is called a **nag loop** or **soft steering**.

Essential rules:

- **Place the nag inside a tool result, NOT a system message** — tool results are read every turn
- **Tag the nag so the model recognizes it as harness-generated** — `<reminder>` is distinct from real tool output
- **Reset the counter rigorously on success**
- **Never stack multiple nags simultaneously**

## 3.9 What the Loop Deliberately Omits

1. **Parallel tool execution** — tool calls run sequentially. For production, dispatch concurrently using `tokio::join!`.

2. **Streaming responses** — the harness waits for the complete response. Streaming adds complexity handling partial tool_use blocks.

3. **Intelligent compaction** — truncation to a fixed window. A sophisticated agent might summarize older turns.

4. **Dynamic tool loading** — tools are passed once and never change.

Each omission represents a natural extension point that adds code and failure modes. `rust_toy_agent` executes real, meaningful tasks without any of them.

---

**Next:** Chapter 4 — Tool Design and Sandboxing