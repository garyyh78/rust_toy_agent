# Chapter 3: The Agent Loop

> "When in doubt, use brute force." — Ken Thompson

Every agent, no matter how sophisticated, boils down to a loop:

```
1. send the conversation history to the model
2. read the model's response
3. if it asked for a tool call, run it and append the result
4. otherwise, stop
5. go to 1
```

That's the whole job of the harness. Chapters 2 through 10 are
essentially elaborations on this loop. In this chapter we pin it
down by reading `src/agent_loop.rs` — the forty-line `agent_loop`
function that holds the entire `rust_toy_agent` together.

## 3.1 The Shape of a Message

Before the loop, the format. Every message the agent sends or
receives is a JSON object with a `role` and a `content`:

```rust
pub type Messages = Vec<Json>;
```

The history is just a vector of these JSON blobs, not a typed
struct. This is a deliberate choice — the Anthropic Messages API
(and OpenAI's equivalent, and every other major LLM API) has been
adding fields to messages for two years, and a fully typed Rust
representation would be outdated before the code shipped. A
`Vec<Json>` is the lazy, right answer.

Roles you will see:

* **`"user"`** — either the human's original prompt or the
  harness reporting tool results. After the first turn, every
  `user` message is actually the harness, not a person.
* **`"assistant"`** — what the model produced. Content is an
  array of *blocks*: text blocks, tool-use blocks, and (on some
  APIs) thinking blocks.
* **`"system"`** — on Anthropic, this lives in a separate
  top-level `system` field rather than as a message. On OpenAI
  it is the first message with `role: "system"`. Chapter 8
  treats system prompts in detail.

A concrete example — two turns of history after one tool call:

```json
[
  {"role": "user", "content": "Count Python files in src/"},
  {"role": "assistant", "content": [
    {"type": "text", "text": "I'll run find."},
    {"type": "tool_use", "id": "tu_01", "name": "bash",
     "input": {"command": "find src -name '*.py' | wc -l"}}
  ]},
  {"role": "user", "content": [
    {"type": "tool_result", "tool_use_id": "tu_01", "content": "42\n"}
  ]}
]
```

Three invariants in that blob, all enforced in code we are
about to read:

1. Every `tool_use` block has a unique `id`.
2. Every `tool_use` is followed *immediately* by a `user` message
   containing a `tool_result` with the matching `tool_use_id`.
3. Ordering within the `content` array is preserved: text first,
   then tool_use, then more text, and so on.

Violate any of these and the API returns a 400 — and the whole
conversation is now poisoned, because the model has no way to
recover from an unpaired tool call in its own history.

## 3.2 The Loop, Unabridged

Here is `agent_loop`, reformatted slightly for the page. Read it
once now; we will dissect it step by step:

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
            // history is corrupt — bail
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

That's the whole agent. Eight parameters, one loop, about fifty
lines of real code. Everything else in `src/` is something this
function calls or something that helps it call something.

## 3.3 Termination: When Does the Agent Stop?

Look at the stopping condition:

```rust
if stop_reason != "tool_use" {
    return (total_input_tokens, total_output_tokens, round as u32);
}
```

The model itself decides when to stop. It does this by producing a
response with any `stop_reason` other than `"tool_use"` — usually
`"end_turn"` (the model thinks it's done) or `"max_tokens"` (the
response hit the output cap). The harness doesn't second-guess; if
the model says it's done, it's done.

This is the single most counter-intuitive thing about tool-use
loops, and it trips up every first-time agent builder. You might
expect a harness to count rounds, watch for specific text
signatures, or look for a magic "I'm finished" tool. None of that
works well. Modern tool-use APIs already distinguish "this response
ends with a tool call and needs another round" from "this response
is my final answer," and the right thing is to trust that signal.

What about safety limits? Look at `subagent.rs`:

```rust
const MAX_SUBAGENT_TURNS: u32 = 30;
// ...
for _ in 0..MAX_SUBAGENT_TURNS {
    // ...
}
```

Subagents get a hard cap. The main agent does not — in
`rust_toy_agent` the human runs `Ctrl+C` if they want to bail. In
a production system you would add a cap here too, sized for the
task: 30 rounds for a focused job, 300 for a long run. The cap is
not a termination signal, it is a **circuit breaker**, and when it
trips you should surface that fact loudly rather than silently.

## 3.4 Step 1: Validate Before You Send

The very first thing inside the loop body is validation:

```rust
if let Some(err) = validate_tool_pairing(messages) {
    // corrupted history, stop
    return (0, 0, 0);
}
```

`validate_tool_pairing` walks the history and checks that every
`assistant` message containing a `tool_use` is immediately followed
by a `user` message containing a matching `tool_result`. If it
isn't, the function bails.

Why do this every round? Because you will write bugs. You will
push a message in the wrong order, forget to append a tool result,
or accidentally drop a block during truncation, and the Anthropic
API will punish you with a 400 and an unhelpful error. Validating
client-side before the expensive call means:

1. Faster feedback — the error points at the offending index in
   your own history, not at a line in the Python SDK's JSON
   encoder.
2. No wasted tokens — a 400 counts against your budget; failing
   early does not.
3. No silent poisoning — if the history is broken, *every*
   subsequent round will 400, and you need to know that on round
   one rather than round fifty.

Modern agent harnesses have dozens of validators like this one,
and they are worth their weight in gold. Treat them as debuggers
that run on every request.

## 3.5 Step 2: Truncate the History

```rust
truncate_messages(messages, 8);
```

Eight rounds, meaning roughly sixteen messages (one assistant,
one user per round), is the in-context window the harness keeps
live. Older messages are dropped.

The implementation has one subtlety:

```rust
let mut cut = messages.len() - target_len;
while cut < messages.len() {
    let prev_is_assistant_tool_use =
        messages[cut - 1]["role"] == "assistant" && /* has tool_use */;
    if prev_is_assistant_tool_use {
        cut += 1;  // don't split a tool_use/tool_result pair
        continue;
    }
    break;
}
messages.drain(1..cut);
```

The loop refuses to cut between a `tool_use` and its `tool_result`.
If the naive cut point would leave a dangling `tool_use` at the
top, it shifts the cut one message later. **Never break pair
invariants just to save tokens.** A model whose history starts with
an unpaired tool_use will loop forever, because every round it
regenerates what it thinks should come next.

Note also: `messages.drain(1..cut)` preserves `messages[0]`, the
original user prompt. That anchor message is load-bearing — it is
the only record of what the user actually asked for, and losing it
makes the model forget the goal. Anchors like this come up again
in Chapter 6 when we talk about context compaction.

## 3.6 Step 3: Call the Model

```rust
let Some(mut response) = call_llm(...).await else {
    return (total_input_tokens, total_output_tokens, round as u32);
};
```

`call_llm` does three things: builds a request body (the subject of
Chapter 7), POSTs it, and logs both the request and the response.
If the call fails after all its retries, `call_llm` returns `None`
and we exit the loop gracefully — returning the tokens we have
spent so the caller can report them. We do not panic, we do not
retry forever, and we do not silently swallow the error.

That `let-else` pattern is the Rust way to write "if this
optional is empty, bail from the enclosing function." In Python
it would be:

```python
response = call_llm(...)
if response is None:
    return (total_input_tokens, total_output_tokens, round)
```

You will see this pattern throughout the agent. The harness is
full of places where "no answer" is a legitimate outcome that
should propagate all the way up.

## 3.7 Step 4: Dispatch the Tool Calls

```rust
let (mut results, used_todo) =
    dispatch_tool_calls(&content, workdir, todo, logger);
```

`dispatch_tool_calls` walks the assistant's content, finds every
`tool_use` block, looks up the tool by name, runs it, and builds a
matching `tool_result` block for each. We cover the tool dispatcher
itself in Chapter 4. For now the important observation is **the
loop handles multiple tool calls per round**.

A modern LLM can and often will return several tool_use blocks in
a single response. For example, on round one it might produce:

```
text: "Let me explore the repo."
tool_use: bash(ls)
tool_use: bash(find . -name '*.toml')
tool_use: read_file(README.md)
```

Three tool calls, one model turn. The harness runs all three (in
the existing code, sequentially — §3.9), collects the results into
a single user message, and sends them back together. Parallelism
inside one round is a free performance win: you spent one LLM
round-trip and got three tool outputs back. Agent frameworks that
force one-tool-per-round leave a lot on the table.

## 3.8 Step 5: The Nag Reminder

```rust
rounds_since_todo = if used_todo { 0 } else { rounds_since_todo + 1 };
maybe_inject_nag(&mut results, rounds_since_todo, logger);
```

Here is a pattern worth knowing by name. `maybe_inject_nag` looks
like this:

```rust
fn maybe_inject_nag(results: &mut [Json], rounds_since_todo: usize, ...) {
    if rounds_since_todo >= 3 && !results.is_empty() {
        if let Some(last) = results.last_mut() {
            if let Some(content) = last["content"].as_str() {
                let updated = format!(
                    "{content}\n\n<reminder>Update your todos.</reminder>"
                );
                last["content"] = json!(updated);
            }
        }
    }
}
```

Every round the agent goes without touching the todo tool, a
counter ticks up. At three, the harness appends a short reminder
to the *next* tool result. The model sees that reminder and —
because it is sitting inside a tool_result block the model already
trusts — updates its plan on the following round.

This is called a **nag loop** or **soft steer**. It is one of the
most powerful techniques in agent engineering, for the same reason
it works on humans: the reminder arrives exactly when the target
is already paying attention, and it piggybacks on a channel the
target treats as authoritative.

Some rules for nag loops, from painful experience:

* **Put the nag inside a tool result, not a system message.** The
  model re-reads the system prompt only occasionally. It reads
  tool results every single turn.
* **Tag the nag so the model recognizes it as harness-generated.**
  The `<reminder>...</reminder>` wrapper is distinct from real
  tool output. A model that sees the same nag twice learns to
  act on it immediately.
* **Reset the counter on success.** The moment the model updates
  its todo list, `rounds_since_todo` goes back to zero. No one
  likes being nagged after they already did the thing.
* **Do not stack nags.** If the agent forgets to update the todo
  list *and* has been writing to `/etc/passwd`, pick the most
  urgent nag and send only that one. Three simultaneous reminders
  read as noise and the model starts ignoring all of them.

Nag loops are how you turn a distractable model into a focused
one without retraining anything. We come back to them in
Chapter 8.

## 3.9 What the Loop Doesn't Do

A few things `agent_loop` deliberately leaves out:

1. **Parallel tool execution.** The tool calls inside one round
   run sequentially. A tool that reads a file and another that
   calls a long `bash` command do not overlap. For a small
   coding agent this is fine; for a production agent with
   ten-second tool calls, you would dispatch them concurrently
   with `tokio::join!` or `FuturesUnordered`. Chapter 8 sketches
   the change.

2. **Streaming responses.** The model produces its output in one
   shot. The harness waits, then reads the full JSON body. A
   streaming client would display tokens as they arrive, which
   is good UX but adds substantial complexity (partial tool_use
   blocks, reconnection logic, backpressure). For a read-only
   tour agent it would not change the architecture.

3. **Compaction.** When the history gets long, this loop
   truncates it to a fixed window. It does not *summarise* the
   dropped turns. A production agent might run a compaction
   pass — ask a cheap model to summarise the older turns into
   one or two synthetic messages — and prepend that summary.
   Chapter 6 covers compaction.

4. **Dynamic tool loading.** The tools vector is passed in once
   and never changes. An advanced harness might swap tools based
   on the task (e.g. hide dangerous tools until the model has
   proven itself, or reveal project-specific tools once the
   workdir is detected). None of these matter for our tour.

Every one of these is a natural extension, but each adds dozens
of lines and a handful of new failure modes. The art is knowing
when you need them — and `rust_toy_agent`, for all its simplicity,
can run real tasks without any of them.

## 3.10 Exercises

1. Trace the conversation history on paper for a two-tool-call
   round. What is in `messages` at the start of the loop? At the
   end? Count the messages before and after.

2. `truncate_messages(messages, 8)` runs on every iteration, even
   when the history is short. Is the cost ever significant?
   Under what conditions?

3. Add a hard cap of 50 rounds to `agent_loop`. When the cap
   trips, the function should push a synthetic tool_result
   saying `<cap>round limit reached</cap>` and then return.
   Why push a result at all rather than just returning?

4. Comment out the call to `validate_tool_pairing` and run the
   test suite. Do any existing tests break? What does that tell
   you about test coverage for validators?

5. Suppose the model starts returning a new `stop_reason` called
   `"pause"` that means "give me the current time and resume."
   Where in the loop would you handle it, and what would the
   response look like?

In Chapter 4 we zoom into `dispatch_tool_calls` and ask the other
hard question in agent engineering: what tools should we expose,
and how should they behave when the model uses them wrong?
