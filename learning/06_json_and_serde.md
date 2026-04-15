# Chapter 6: Context Management — Making Long Runs Possible

> "Programs must be written for people to read, and only incidentally for machines to execute." — Abelson and Sussman

The context window is the stage on which the whole agent performs.
Everything the model knows about the current task lives in that
window: the user's goal, every tool call it has made, every result
it has received, the todo list, the system prompt. When the window
fills up, the agent dies — either instantly (API rejects the
request) or slowly (the model starts forgetting the point of the
task). Both failure modes are ugly, and both are preventable.

This chapter is about managing the window. We read the truncation
logic in `src/agent_loop.rs` and the compaction pipeline in
`src/context_compact.rs`, and along the way we derive the four
laws of long-running agent memory.

## 6.1 The Numbers to Remember

Let's anchor the discussion in real costs. For a typical modern
coding agent:

| thing | size |
| --- | --- |
| context window | 200,000 tokens |
| a read of `src/main.rs` | 500–2000 tokens |
| one `tool_result` block | 50–2000 tokens |
| a full round (model + result) | 200–4000 tokens |
| price per 1M input tokens | $3 |
| price per 1M output tokens | $15 |

A hundred-round session, naively, sends 100 × 2000 = 200,000
tokens per round on average, times 100 rounds = 20 million input
tokens — $60 in API costs for one task, if the whole history is
resent every round. A thousand-round session is $6000. At that
point the economics are not "clever optimization" — they are the
difference between a viable product and an unshippable one.

Context management is the engineering that makes the difference.

## 6.2 The Four Laws

Before the code, four rules that every technique in this chapter
is a specialization of:

1. **Never resend what hasn't changed.** Prompt caching (we meet
   it shortly) lets the API remember the static parts of your
   request and charge you a fraction of the price for them.
2. **Drop what the model no longer needs.** Old tool results,
   exploratory reads, branches of reasoning that were abandoned —
   none of these help the model finish, and each one costs real
   money.
3. **Summarize what you cannot drop.** When the history has too
   much signal to throw away but too little space to keep,
   replace the details with a synthesis.
4. **Never break invariants to save tokens.** A tool_use without
   its matching tool_result is worse than a bloated history,
   because the API rejects it outright.

Every line of code in this chapter is one of those four laws in
action.

## 6.3 The Simple Truncator

Start with the simplest technique. Here again is
`truncate_messages` from `agent_loop.rs`:

```rust
fn truncate_messages(messages: &mut Messages, max_rounds: usize) {
    let each_round = 2;
    let target_len = 1 + max_rounds * each_round;
    if messages.len() <= target_len {
        return;
    }
    let mut cut = messages.len() - target_len;
    while cut < messages.len() {
        let prev_is_assistant_tool_use = messages[cut - 1]["role"] == "assistant"
            && messages[cut - 1]["content"]
                .as_array()
                .map(|arr| arr.iter().any(|b| b["type"] == "tool_use"))
                .unwrap_or(false);
        if prev_is_assistant_tool_use {
            cut += 1;
            continue;
        }
        break;
    }
    messages.drain(1..cut);
}
```

The algorithm:

1. Decide on a target history length: the first message (the
   user's original prompt), plus `max_rounds` rounds, where a
   round is one assistant message and one user message.
2. If we're under the target, do nothing.
3. Otherwise, drain the oldest messages *after* the first —
   `messages.drain(1..cut)` — to get back to the target.
4. Before draining, step forward past any cut point that would
   leave a `tool_use` without its `tool_result`.

This is Law 2 (drop) and Law 4 (preserve invariants) working
together. The function does not summarize; it just forgets. It
also does not touch the first message, which is Law 2's big
exception: the user's original prompt is the anchor that tells
the model what it is even doing. **Always keep the anchor.**

When is a pure truncator enough? When your typical task takes
fewer than 2–3 × `max_rounds` turns, the dropped history is
genuinely not needed. `rust_toy_agent` sets `max_rounds = 8`, so
the agent keeps the last 8 rounds of tool calls plus the original
prompt. For coding tasks of the "find this bug, fix it, verify"
variety, that is plenty.

## 6.4 When Truncation Is Not Enough

Truncation fails on tasks that need to remember *earlier* state.
Three scenarios:

1. **Exploratory reads.** The model opens twelve files early on
   to understand the codebase, then builds its fix in the last
   three rounds. Truncate to 8 rounds and the fix round no longer
   has the exploratory context — the model is working from
   memory, badly.

2. **Long bug hunts.** The model tries six hypotheses, each one a
   few rounds, and finds the bug on hypothesis seven. Truncation
   drops the failed hypotheses and the model re-tries some of
   them because it no longer remembers that it already did.

3. **Multi-step plans with persistent state.** "Edit every file
   that imports `foo`." The model tracks progress in the todo
   list, but the *edits themselves* need to be recorded somewhere
   the model can look back at.

For scenarios like these, `rust_toy_agent` ships a second
mechanism: **context compaction**.

## 6.5 Compaction: Keeping the Signal, Dropping the Noise

`context_compact.rs` implements a three-layer compression
pipeline. Start with the header comment, which is the clearest
explanation in the whole file:

```rust
/// Layer 1: micro_compact - replace old tool results with placeholders
/// Layer 2: auto_compact - save transcript, summarize, replace messages
/// Layer 3: manual_compact - triggered by compact tool
```

Three strategies, increasing in aggressiveness. Each layer
corresponds to a different trade-off between preservation and
compression.

### Layer 1: Micro-compact

The simplest non-trivial technique. It walks the history,
identifies old `tool_result` blocks (older than `keep_recent = 3`
rounds), and replaces their bodies with a one-line placeholder:

```
[tool_result from bash(git status), 142 bytes, see round #4 in transcript]
```

The model keeps the structure — it still knows a tool call
happened and roughly what it was — but the full content is gone.
For reads that were informational at the time and are no longer
load-bearing, this is almost lossless.

The key insight: **tool results decay.** A fresh tool_result is
precious: the model is about to reason over it. A tool_result
from ten rounds ago has probably already been *used* — the model
pulled the signal out and used it to write some edit. Keeping the
full text is paying for data the model has already metabolized.

Micro-compact is cheap — it runs locally, no extra LLM call — and
typically saves 30–60% of the token budget in tool-heavy
sessions. It should run on most rounds of any long session.

### Layer 2: Auto-compact

When micro-compact is not enough — when the total token estimate
crosses `COMPACT_THRESHOLD` — the harness fires a heavier pass.
The rough flow:

1. **Save the full conversation to disk** as a transcript. This
   is the bailout valve: if the compaction summary is bad, a
   human can still recover the original.
2. **Call a cheap model** (or the same model, depending on the
   harness) with a prompt like: *"Summarize the following
   conversation in a form the agent can use to continue its
   task. Preserve file paths, function names, and partial
   progress. Drop exploratory reads and redundant observations."*
3. **Replace the dropped messages** with a synthetic
   `tool_result` block containing the summary. The *recent*
   rounds stay verbatim.
4. **Re-enter the loop** with the compacted history.

This is Law 3 (summarize) in action. Three non-obvious details:

* **The summary is delivered as a tool_result, not as a system
  message.** Tool_results are on the model's "things I read every
  turn" list; system messages drift into the background. If you
  want the summary to shape the next action, put it where the
  model looks.
* **Recent rounds are kept verbatim.** The model is mid-thought
  on the last few turns, and paraphrasing those would corrupt the
  reasoning. Compact the tail of history, never the head of
  "right now."
* **The dropped transcript is saved to disk.** If the compaction
  summary is wrong and the agent gets stuck, you can inspect what
  was lost. Never throw away data without somewhere to look it up.

The cost of auto-compact is one extra LLM call — typically under
a second — and it buys you a 5–10× reduction in token usage for
the rest of the session. On long runs the savings are
overwhelming.

### Layer 3: Manual compact

The third layer is manual: a tool the model itself can call,
named `compact`, whose only purpose is to say "I'm about to start
a new phase, please compress what I've done so far." This is the
agent-engineering equivalent of "clear the desk before starting
fresh work."

Exposing compaction as a tool the model can call is unusual and
worth understanding. Two reasons it works:

1. **The model knows its own state better than the harness.** A
   timer-based heuristic ("compact every 30 rounds") is blind to
   task structure. The model knows when it has finished the
   explore phase and is starting the edit phase, and that is the
   right moment to compress.

2. **The model is trained to use tools.** A `compact` tool is
   just another tool with another description. The model picks
   it up naturally, without any change to the system prompt.

This pattern — **expose harness capabilities as tools** —
generalizes far beyond compaction. Need the model to be able to
launch subagents? Expose `subagent` as a tool. Need it to be able
to queue a background job? Expose `background_task`. The tool
interface is the API between the model and your harness, and it
scales.

## 6.6 Prompt Caching: The Free Lunch

One more technique, and it is the most impactful of all. Modern
LLM APIs support **prompt caching**: you mark the stable portion
of your request — usually the system prompt plus the tool
definitions — and the API caches the processed state. Subsequent
requests with the same stable prefix hit the cache and cost a
fraction of the normal price. On Anthropic, a cache hit is 10%
of a cache miss on input, and the cache lives for 5 minutes.

Concretely: a system prompt of 2000 tokens plus a tool definition
of 3000 tokens is 5000 tokens. On a 100-round session, that's
500,000 tokens of *stable* content sent 100 times. Without
caching: 500,000 × $3/M = $1.50. With caching: first call costs
$0.02 extra, 99 cache hits cost $0.15 total, for around $0.17.
Almost 10× cheaper, for nothing but annotating the request.

`rust_toy_agent` does not use prompt caching yet — the header
comments note it as a TODO — but any production agent you ship
should. The whole feature boils down to adding
`"cache_control": {"type": "ephemeral"}` to one or two blocks in
the request body. Read the API docs once; that ten minutes pays
for itself on your first real run.

The practical rules of caching:

* **Cache breakpoints go at stable boundaries.** System prompt
  end, tools end, first user message — those are the places
  content stops varying.
* **Reorder for cache locality.** Put stable content first,
  volatile content last. A request that puts the date at the
  very top will cache nothing.
* **Cache hits only stay warm if you re-send within the TTL.**
  On Anthropic that's 5 minutes by default (1 hour with the
  extended option). Long human coffee breaks cost cache.

Caching is Law 1 — "never resend what hasn't changed" — made
concrete. It is the single highest-leverage change you can make
to an agent's bill.

## 6.7 Pairing Invariants Revisited

Every technique in this chapter has to respect the rule from
Chapter 3: `tool_use` and its matching `tool_result` are an
inseparable pair. When you truncate, you truncate pairs, not
single messages. When you compact, you replace pairs with
synthetic pairs. When you cache, you cache entire messages, not
halves of them.

Breaking this invariant produces one of the most frustrating bugs
in agent development: the request goes out, the API returns 400
with a generic "unexpected message structure," and you have no
idea which pair is broken. The fix, always, is to re-run the
validator from `validate_tool_pairing` after every context
operation. Make it a habit.

## 6.8 What a Well-Managed Session Looks Like

Put it all together. A long-running session in `rust_toy_agent`
follows roughly this arc:

```
Rounds 1-8:   Full history, no compaction. Trivial truncator
              is a no-op. Cache warms up on round 2.
Rounds 9-30:  Truncator starts dropping oldest rounds. Cache
              hits every round. Token usage flat.
Rounds 30+:   Micro-compact kicks in. Old tool results become
              one-line placeholders. Token usage drops ~40%.
Round 60:     Estimated tokens cross the compact threshold.
              Auto-compact fires: summarize rounds 1-50, keep
              50-60 verbatim. History shrinks from ~80K tokens
              to ~20K. Cache resets.
Rounds 60+:   Truncator + micro-compact resume. Tokens grow
              slowly again until the next auto-compact.
```

The shape is sawtooth: steady growth, then a sharp drop, then
steady growth again. Plot token usage of any well-behaved
production agent and you will see this curve. If you see the
curve going straight up to the context window limit, your agent
has no compaction and is about to die; if you see it flatlined
near 80% of the window, you have compaction but no caching and
are paying too much; if you see the sawtooth, you are shipping.

## 6.9 Exercises

1. In `agent_loop.rs`, `truncate_messages(messages, 8)` runs
   unconditionally every round. Measure how often it actually
   drops a message. For short tasks, is it mostly a no-op?

2. The micro-compact keeps the three most recent tool results
   verbatim. Change it to keep just one. Run a real task.
   Does the agent behave noticeably worse? Measure token
   savings.

3. Suppose you add prompt caching to `build_request_body`
   (Chapter 7). Where should the cache breakpoint go: after
   the system prompt, after the tool definitions, both, or
   before each user message? Argue each choice.

4. Read `estimate_tokens` in `context_compact.rs`. It uses
   4 chars = 1 token as a rough estimate. When is this
   estimate wrong? Would you use the exact tokenizer
   instead, or is the approximation good enough?

5. Design a fourth compaction layer: `differential_compact`.
   When the same file has been read twice, keep only the
   second read and note "see round X for earlier version."
   Does this save tokens in the typical case? Does it lose
   information the model might need?

In Chapter 7 we zoom into `llm_client.rs` and ask what happens
when the network, inevitably, breaks mid-request.
