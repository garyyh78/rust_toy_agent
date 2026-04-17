# Chapter 2: Working Memory — Giving the Agent a Scratchpad

> "Controlling complexity is the essence of computer programming." — *Brian Kernighan*

## Introduction

A language model is fundamentally stateless: you provide a prompt, it returns a response, and the next call remembers **nothing** unless you paste the previous turn back in. Everything called "agent memory" is an illusion the harness constructs by making deliberate decisions about what to include in each prompt.

This chapter explores the simplest piece of that illusion: a **todo list** the model can both read and write independently.

## 2.1 The Core Problem: Models Forget What They Said

Consider a task: "Migrate the user service from PostgreSQL to SQLite."

A careful developer breaks this into substeps, checks them off mentally. What does a model do left to its own devices?

- **Round 1:** "First, I will read the schema."
- **Round 2:** "Now let me find the user service file..."
- **Round 3:** "Let me check the migrations directory..."
- **Round 4:** "Let me think about the overall plan..." — the plan is **subtly different** from Round 1 because the context window filled with tool outputs and the model reconstructs its intent from accumulated evidence.

This failure mode is called **plan drift**. The solution is an **externalized plan**: a structured list the model writes into and reads back from every single round.

## 2.2 The Complete Shape of the Scratchpad

```rust
const MAX_TODO_ITEMS: usize = 20;

pub struct TodoItem {
    pub id: String,
    pub text: String,
    pub status: String,   // "pending" | "in_progress" | "completed"
}

pub struct TodoManager {
    items: Vec<TodoItem>,
}
```

Design choices:

- **`id` is a string, not an auto-incrementing integer.** Human-readable IDs like `"schema-port"` or `"1"` survive paraphrasing. An auto-incrementing integer would force the harness to perform complex list diffing.

- **`status` accepts exactly three values.** Not a boolean `done` flag, not a fancy enum. The three values — `pending`, `in_progress`, `completed` — match a vocabulary every model knows. Adding a fourth state is almost always wrong until you have production data showing the model actually uses it.

- **`MAX_TODO_ITEMS = 20`.** A plan with more than twenty items is almost certainly a plan that won't finish successfully. This hard cap forces the model to decompose hard problems into sub-tasks.

- **No nesting, no priority ranking, no assignee field, no deadline.** The model is the sole worker and the harness is the sole scheduler. Every field must pay its rent.

## 2.3 The Invariants

```rust
if items_json.len() > MAX_TODO_ITEMS {
    return Err(format!("Max {MAX_TODO_ITEMS} todos allowed"));
}
if text.is_empty() {
    return Err(format!("Item {item_id}: text required"));
}
if !matches!(status.as_str(), "pending" | "in_progress" | "completed") {
    return Err(format!("Item {item_id}: invalid status '{status}'"));
}
if in_progress_count > 1 {
    return Err("Only one task can be in_progress at a time".to_string());
}
```

These rules fight specific failure modes:

| Rule | Failure Mode Prevented |
|------|----------------------|
| Cap the size | Burning excessive tokens |
| Reject empty text | Unreadable rendered lists |
| Enum the status | Silent miscounting in rendering |
| Exactly one `in_progress` | "Plan-hopping" between tasks |

**What is NOT enforced:** you can mark a completed task back to `pending`, freely reorder items, and delete half the list. These are things humans do routinely.

The rule of thumb: **enforce the things that break the system, permit the things that look untidy.**

## 2.4 Update Semantics: Replace Rather Than Diff

```rust
self.items = validated;
Ok(self.render())
```

The **entire new list** completely replaces the old one. This is deliberate:

1. **Models excel at producing lists but struggle with diffs.** Asking the model to generate a JSON patch is a recipe for subtle, hard-to-debug errors.

2. **Full-replace is idempotent.** If the network flakes and a tool call executes twice, the final state is identical.

3. **The rendered new state is returned as the tool result.** The model sees exactly what the harness now believes — an anti-drift technique.

The cost is fifty additional tokens per update; the benefit is a completely reliable scratchpad that never drifts.

## 2.5 The Rendering Format

The model never sees `Vec<TodoItem>` directly. It sees whatever string the harness renders:

```
[ ] #1: Write failing test
[>] #2: Port schema
[x] #3: Read existing migrations
(1/3 completed)
```

Design principles:

- **Status glyphs are distinctive:** `[ ]`, `[>]`, `[x]` — three bracketed characters. Do NOT use emoji — they tokenize into multiple tokens and render inconsistently.

- **The ID is prefixed with `#`.** Makes the ID visually distinct from text content.

- **The progress line is absolute, not percentage-based:** `(1/3 completed)` tells the model both remaining count AND total scale. Percentages hide the actual scale.

- **Empty lists render as `No todos.`,** not a blank string. Empty strings deeply confuse models.

## 2.6 What State Lives Where

| State Type | Lives In | Accessible to Model |
|-----------|----------|---------------------|
| Conversation history | `Messages` vec | **Yes**, always |
| Todo list | `TodoManager` | **On demand**, via tool |
| Working directory | `WorkdirRoot` | **Implicitly** in tool results |
| Background tasks | `BackgroundManager` | **On demand**, via tool |
| Session log | `SessionLogger` | **No** — human only |

The rule for state placement: **if the model needs to read it on EVERY turn, put it in the system prompt. If the model needs to read it OCCASIONALLY, expose it through a tool. If the model NEVER needs to read it, keep it inside the harness.**

## 2.7 Testing Strategy

`todo_manager.rs` contains seventeen unit tests. They run in under a single millisecond without touching the network, filesystem, or LLM.

Agent harness systems require two types of tests:

1. **Fast, deterministic tests** for pure logic: state validators, path sanitizers, renderers. These catch ~80% of regressions.

2. **Slow, expensive tests** for end-to-end behavior: requiring a real LLM and complete scoring methodology. These run nightly at best.

The crucial skill is making pure pieces genuinely pure. A `TodoManager` whose `update` method needed network access would be a nightmare to test. A `TodoManager` whose `update` is a pure function is trivial to test.

---

**Next:** Chapter 3 — The Agent Loop