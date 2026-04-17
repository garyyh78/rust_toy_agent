# Chapter 6: Context Management — Making Long Runs Possible

> "Programs must be written for people to read, and only incidentally for machines to execute." — *Abelson and Sussman*

## Introduction

The context window is the stage on which the whole agent performs. Everything the model knows about the current task lives in that window. When it fills up, the agent dies — either instantly (API rejects request) or slowly (model forgets the task).

This chapter covers managing the window: truncation logic in `src/agent_loop.rs` and the compaction pipeline in `src/context_compact.rs`.

## 6.1 The Numbers

| Thing | Size |
|-------|------|
| Context window | 200,000 tokens |
| Read of `src/main.rs` | 500–2000 tokens |
| One `tool_result` block | 50–2000 tokens |
| A full round | 200–4000 tokens |
| Price per 1M input tokens | $3 |
| Price per 1M output tokens | $15 |

A hundred-round session sends 100 × 2000 = 200,000 tokens per round on average. 100 rounds = 20 million input tokens — $60 in API costs. Context management is the engineering that makes long runs viable.

## 6.2 The Four Laws

1. **Never resend what hasn't changed.** Prompt caching lets the API remember static parts and charge a fraction.

2. **Drop what the model no longer needs.** Old tool results, exploratory reads, abandoned branches — none help finish the task.

3. **Summarize what you cannot drop.** When history has too much signal to throw away but too little space, replace details with synthesis.

4. **Never break invariants to save tokens.** A tool_use without matching tool_result is worse than a bloated history.

## 6.3 The Simple Truncator

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

Algorithm:

1. Target history length = first message + max_rounds × 2
2. If under target, do nothing
3. Otherwise, drain oldest messages after the first
4. Before draining, step forward past any cut point that leaves tool_use without tool_result

`rust_toy_agent` sets `max_rounds = 8`. For "find this bug, fix it, verify" tasks, that's plenty.

## 6.4 When Truncation Is Not Enough

Truncation fails on tasks that need to remember earlier state:

1. **Exploratory reads** — model opens twelve files early, builds fix in last three rounds. Truncate and fix round loses exploratory context.

2. **Long bug hunts** — model tries six hypotheses, finds bug on seventh. Truncation drops failed hypotheses and model re-tries some.

3. **Multi-step plans with persistent state** — "Edit every file that imports `foo`". Edits need to be recorded somewhere the model can look back at.

For these scenarios, `rust_toy_agent` ships context compaction.

## 6.5 Compaction: Keeping the Signal

`context_compact.rs` implements a three-layer pipeline:

```rust
/// Layer 1: micro_compact - replace old tool results with placeholders
/// Layer 2: auto_compact - save transcript, summarize, replace messages
/// Layer 3: manual_compact - triggered by compact tool
```

### Layer 1: Micro-compact

Walks history, identifies old tool_result blocks (older than `keep_recent = 3` rounds), replaces with:

```
[tool_result from bash(git status), 142 bytes, see round #4 in transcript]
```

The model keeps the structure but content is gone. Tool results decay — a fresh tool_result is precious; one from ten rounds ago has probably already been used.

Micro-compact runs locally, no extra LLM call, typically saves 30–60% of token budget.

### Layer 2: Auto-compact

When total token estimate crosses `COMPACT_THRESHOLD`:

1. **Save full conversation to disk** as transcript — the bailout valve
2. **Call a cheap model** with prompt: *"Summarize the following conversation in a form the agent can use to continue its task. Preserve file paths, function names, and partial progress."*
3. **Replace dropped messages** with synthetic tool_result containing summary. Recent rounds stay verbatim.
4. **Re-enter the loop** with compacted history

Three non-obvious details:

- **Summary delivered as tool_result, not system message** — tool_results are on model's "things I read every turn" list
- **Recent rounds kept verbatim** — model is mid-thought; paraphrasing would corrupt reasoning
- **Transcript saved to disk** — if compaction is wrong, human can recover original

### Layer 3: Manual Compact

A tool the model itself can call: `compact`, whose purpose is "I'm about to start a new phase, please compress what I've done so far."

Exposing compaction as a tool works because:

1. **Model knows its own state better than harness** — timer-based heuristic is blind to task structure
2. **Model is trained to use tools** — picks it up naturally

## 6.6 Prompt Caching: The Free Lunch

Modern LLM APIs support prompt caching: mark the stable portion (system prompt + tool definitions) and the API caches the processed state. On Anthropic, cache hit is 10% of cache miss, cache lives 5 minutes.

A system prompt of 2000 tokens + 3000 token tool definition = 5000 tokens. On 100-round session:

- Without caching: 500,000 × $3/M = $1.50
- With caching: first call costs $0.02 extra, 99 cache hits cost $0.15 total ≈ $0.17

Almost 10× cheaper for nothing but annotating the request.

`rust_toy_agent` notes it as a TODO. Any production agent should implement it.

Practical rules:

- **Cache breakpoints at stable boundaries** — system prompt end, tools end
- **Reorder for cache locality** — put stable content first
- **Re-send within TTL** — on Anthropic that's 5 minutes default

## 6.7 Pairing Invariants Revisited

Every technique must respect: tool_use and matching tool_result are an inseparable pair. When you truncate, truncate pairs. When you compact, replace pairs. When you cache, cache entire messages.

Breaking this invariant produces the most frustrating bug: request goes out, API returns 400 "unexpected message structure," no idea which pair is broken. Fix: re-run validator after every context operation.

## 6.8 What a Well-Managed Session Looks Like

```
Rounds 1-8:   Full history, no compaction. Cache warms up on round 2.
Rounds 9-30:  Truncator drops oldest rounds. Cache hits every round.
Rounds 30+:   Micro-compact kicks in. Token usage drops ~40%.
Round 60:     Auto-compact fires: summarize rounds 1-50, keep 50-60.
Rounds 60+:   Truncator + micro-compact resume. Tokens grow slowly again.
```

The shape is sawtooth: steady growth, sharp drop, steady growth. Plot token usage and you will see this curve.

---

**Next:** Chapter 7 — Robust LLM I/O