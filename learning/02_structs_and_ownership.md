# Chapter 2: Working Memory — Giving the Agent a Scratchpad

> **"Controlling complexity is the essence of computer programming."** — *Brian Kernighan*

---

## Introduction: The Memory Problem

A large language model, for the purposes of building an agent, is fundamentally a **stateless function**: you provide it a prompt, it returns a response, and the very next call remembers **nothing** unless you explicitly paste the previous turn back into the input window. Everything we call "agent memory" is actually an **illusion** that the harness constructs from the outside by making deliberate, strategic decisions about exactly what to include in each successive prompt.

This chapter explores the simplest yet most undervalued piece of that illusion: a **todo list that the model can both read and write independently**. This is not a lesson about data structures in the abstract — it is a practical lesson about what an agent genuinely needs to remain effective across dozens of tool calls, why free-form chain-of-thought reasoning alone is insufficient, and how a fifty-line Rust module can fundamentally transform your agent's behavior from appearing to "flail randomly" to operating with clear "purpose and direction."

The specific code we will examine lives in `src/todo_manager.rs`. You glimpsed one of its types in Chapter 1. Now we will understand **exactly why it exists** and **how it solves** the core memory challenge.

---

## 2.1 The Core Problem: Models Forget What They Said

Let us paint a vivid picture of a realistic agent task: *"Migrate the user service from PostgreSQL to SQLite and update all integration tests accordingly."*

A careful human developer would break this into a half-dozen distinct substeps — locate the service file, read the database schema, create porting migrations, handle SQL dialect differences, stub out incompatible features, run the test suite, diagnose any failures, and fix them methodically. They would check each substep off mentally (or on paper) as they complete it.

**What does a model do when left to its own devices?**

A model does something considerably worse than simply forgetting: it **improvises a new plan on every single turn**. Consider this very realistic interaction:

- **Round 1:** "First, I will read the schema to understand the current structure."
- **Round 2:** "Now let me find the user service file..."
- **Round 3:** "Let me check the migrations directory..."
- **Round 4:** "I'll examine the test files to understand what needs updating..."
- **Round 5:** "Let me think about the overall plan..." — and the plan it produces is **subtly different** from the one it described in Round 1, because the context window has filled up with tool outputs and the model is now reconstructing its own intent purely from the accumulated evidence.

This failure mode has an established name in the agent-engineering community: **plan drift**. It is arguably the single biggest reason why long-running agents abandon tasks, repeat themselves redundantly, or ship incomplete work. The solution is absolutely not a smarter model — it is an **externalized plan**: a structured list that the model writes into and reads back from, on every single round, with the same deliberate ceremony it uses to invoke any other tool.

This is precisely what `TodoManager` accomplishes. It serves as the agent's **external scratchpad**, owned and managed by the harness, surfaced through a well-designed tool interface.

---

## 2.2 The Complete Shape of the Scratchpad

Here is the **entire type definition** with all its supporting infrastructure:

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

Let us analyze **every design choice** with color-coded insights:

### Field Design Philosophy

- **`id` is a string, not an auto-incrementing integer** (highlighted in **blue**). The model assigns these IDs when it constructs the list. Human-readable IDs like `"schema-port"` or `"1"` survive **paraphrasing and partial re-planning** gracefully. An auto-incrementing integer would force the harness to perform complex list diffing — and that bookkeeping complexity is exactly the kind of work we want the model to handle for us actively.

- **`status` is a string accepting exactly three values** (highlighted in **green**). This is deliberately **not** a boolean `done` flag, and it is **not** a fancy `Done | Todo | Doing` enum. The three values — `pending`, `in_progress`, and `completed` — match a vocabulary that **every language model already knows intimately**. Adding a fourth state — for instance, `"blocked"` — is TEMPTING, but it is almost always WRONG until you have concrete production data showing the model actually wants and uses it. **Agent vocabularies should be chosen carefully once and then vigorously defended against feature creep.**

- **`MAX_TODO_ITEMS = 20`** (highlighted in **purple**). This is absolutely NOT a magic number pulled from the air — it represents a concrete statement about **human (and AI) attention span**. A plan comprising more than twenty items is almost certainly a plan that the model will not finish successfully, and almost always a plan that should have been decomposed into two separate plans. This hard cap forces the model to break genuinely hard problems into smaller sub-tasks with their own dedicated plans — typically through the use of **subagents**, which we explore thoroughly in Chapter 9.

- **There is absolutely no nesting, no priority ranking, no assignee field, and no deadline** (highlighted in **orange**). A todo list for a human project manager typically includes all of those sophisticated features. An agent todo list deliberately omits them all because the model is the sole worker AND the harness is the sole scheduler. Every single field that exists **must pay its rent**: it must be something the model will read, write, or actively reason about on a meaningful percentage of turns. Fields that sound theoretically useful but go unused in practice are simply wasted tokens that the model has to unnecessarily scan past on every single round.

---

## 2.3 The Invariants: Rules That Keep the Agent Honest

Let us examine the critical validation loop from the `update` method, with detailed explanations:

```rust
if items_json.len() > MAX_TODO_ITEMS {
    return Err(format!("Max {MAX_TODO_ITEMS} todos allowed"));
}
// ...
if text.is_empty() {
    return Err(format!("Item {item_id}: text required"));
}
if !matches!(status.as_str(), "pending" | "in_progress" | "completed") {
    return Err(format!("Item {item_id}: invalid status '{status}'"));
}
// ...
if in_progress_count > 1 {
    return Err("Only one task can be in_progress at a time".to_string());
}
```

These **four rules** exist for specific, identifiable reasons — each one fights a particular failure mode that we have observed in real deployed agents:

| Rule | Purpose | Failure Mode Prevented |
|------|---------|-------------------|
| **Cap the size** | Prevents the model from turning the todo list into a comprehensive brain dump | Burning excessive tokens on every round |
| **Reject empty text** | Stops placeholder rows like `{id: "2", text: "", status: "pending"}` | Unreadable rendered lists |
| **Enum the status** | Prevents the model from inventing new states like `"doing"` vs `"in-progress"` | Silent miscounting in rendering |
| **Exactly one `in_progress`** | Enforces single-tasking discipline | "Plan-hopping" between tasks |

### Why These Rules Matter

- **Enforcing the size limit** stops the model from creating an overwhelming list and burning tokens on it every single round. When the list grows past twenty items, the model can no longer reason effectively about the entire plan.

- **Rejecting empty text** prevents the model from creating placeholder rows that it intends to fill in later — an easy habit to fall into, and one that renders the list completely unreadable for human reviewers.

- **Enumerating the status** prevents the model from inventing states. If you permit the model to write `status: "doing"`, it will — and then on round seventeen it will write `"in-progress"`, and your renderer will **silently miscount** which tasks are actually in progress. An explicit enum check with a useful error message teaches the model the correct vocabulary **within one round**.

- **Enforcing exactly one `in_progress`** is the most critical rule. It enforces **single-tasking**. A well-behaved agent has exactly one task genuinely in flight at any moment and knows explicitly what that task is. The moment you permit two concurrent in-progress tasks, the model begins **plan-hopping** — jumping between half-finished tasks as it notices new subproblems — and the entire interaction becomes an tangled mess.

### What Is NOT Enforced

Notice carefully what is **not** enforced: you absolutely **can** mark a `completed` task back to `pending`, you **can** freely reorder items, and you **can** delete half the list whenever you want. These are all things that humans do routinely during active planning, and **forbidding** them would force the model into awkward, unnatural workarounds.

The practical rule of thumb for designing invariants on an **agent-facing data structure** is straightforward:

> **Enforce the things that genuinely break the system, but permit the things that simply look untidy.**

---

## 2.4 Update Semantics: Replace Rather Than Diff

Examine carefully the final lines of the `update` method:

```rust
self.items = validated;
Ok(self.render())
```

The **entire new list** completely replaces the old one. The model does NOT send incremental changes like `{add: [...], remove: [...]}`, and it does NOT send per-item patches. Instead, it sends the **complete list** it wants the state to become, every single time.

This is absolutely a **deliberate and important architectural choice**, backed by three distinct rationales:

### Why Full-Replace Is Superior

1. **Models excel at producing lists but struggle enormously with diffs.** Asking the model to generate a JSON patch against a list it remembers from three rounds ago is a recipe for **subtle, hard-to-debug** errors. Asking the model to simply produce the complete list it wants is something every model does reliably from its very first day of training.

2. **A full-replace tool is inherently idempotent.** If the network flakes and a tool call executes twice, the final state remains identical. Agents should **actively aim for idempotent tools** wherever possible — Chapter 5 leans on this principle heavily when designing robust tool interfaces.

3. **The rendered new state is returned as the tool result.** The model sees exactly what the harness now believes, and the next round's context contains the **canonical rendering** rather than the model's potentially faulty memory of what it intended to write. This is a small but powerful **anti-drift technique** that punches enormously above its apparent weight.

### The Real-World Cost

The genuine cost is that the model has to retype the **entire list** each round it changes absolutely anything. In practical terms, this costs perhaps **fifty additional tokens per update** and buys you a **completely reliable scratchpad** that never drifts. That trade-off is absolutely worth it.

---

## 2.5 The Rendering Format: Why Presentation Matters Enormously

The model will **never** see `Vec<TodoItem>` directly. It sees whatever string the harness **renders**, through the specific formatting choice we made. Here is the actual rendered output:

```
[ ] #1: Write failing test
[>] #2: Port schema
[x] #3: Read existing migrations
(1/3 completed)
```

This rendered string is the **only part of `TodoManager`** that the model ever interacts with visually. Let us analyze precisely what it does — and what it deliberately chooses NOT to do:

### Rendering Design Principles

- **Status glyphs are distinctive and unambiguous** (highlighted in **blue**): `[ ]`, `[>]`, `[x]` — three carefully bracketed characters. They do **not** collide with anything the model is likely to encounter in file contents, and they scan beautifully vertically down the list. A crucial rule: **do NOT use emoji** — they tokenize into multiple tokens, render inconsistently across different terminals, and sometimes **trigger the model to reply in emoji**, which Nobody Wants.

- **The ID is prefixed with `#`** (highlighted in **green**). Not because the harness requires the `#`, but because it makes the ID **visually distinct** from the text content. Additionally, when the model references `#3` in its reasoning, it is unequivocally pointing at a specific todo item rather than a section heading or GitHub issue number.

- **The progress line is absolute, not percentage-based** (highlighted in **purple**): `(1/3 completed)` is strictly superior to `(33%)` because the model reads it and immediately understands both the **remaining count** AND the **total scale** of work. Percentages cleverly hide the actual scale of the work remaining.

- **Empty lists render as `No todos.`** (highlighted in **orange**), not as a blank string. Empty strings deeply confuse models — they wonder whether the tool ran correctly at all, whether the previous state is still in effect, whether they need to create a list from scratch. A three-word explicit message answers ALL of those questions definitively.

### The Professional Truth

These are genuinely **tiny, seemingly insignificant decisions**. They are ALSO the specific decisions that separate an agent that uses the todo tool **naturally and effectively** from an agent that requires constant cajoling and prompting. **Spend the extra ten minutes designing a thoughtful render function** — it is almost always the single highest-leverage piece of code in the entire module, affecting every single round of interaction.

---

## 2.6 What State Lives Where — and What Does Not

The todo list is **not** the agent's only form of working memory. It is ONE of several distinct pieces of state that the harness tracks alongside the conversation:

| State Type | Lives In | Session Lifetime | Visibly Accessible to Model |
|-----------|----------|-----------------|---------------------------|
| Conversation history | `Messages` vec | Entire session | **Yes**, always visible |
| Todo list | `TodoManager` | Entire session | **On demand**, via dedicated tool |
| Working directory | `WorkdirRoot` | Entire session | **Implicitly** in all tool results |
| Background tasks | `BackgroundManager` | Entire session | **On demand**, via dedicated tool |
| Session log | `SessionLogger` | Entire session | **No** — human viewers only |

### The Critical Rule for State Placement

Use this **invaluable heuristic** for deciding where any new piece of state belongs:

> **"If the model needs to read it on EVERY turn, put it directly in the system prompt. If the model needs to read it OCCASIONALLY, expose it through a dedicated tool call. If the model NEVER needs to read it, keep it entirely inside the harness and only log it for human reviewers."**

By this rule, `TodoManager` occupies the **middle category**. The todo list is relevant on **most** turns but certainly NOT all — the model specifically requests it when actively planning and updates it when task statuses genuinely change. Burning precious system-prompt tokens to display the list on **every** call would be extraordinarily wasteful — worse, it would actively encourage the model to **NOT update the list**, because the state would feel artificially "pushed" rather than genuinely "owned."

**Tools are unequivocally the natural home** for pull-based state like the todo manager.

---

## 2.7 Comprehensive Testing Strategy

Scroll to the very bottom of `todo_manager.rs` and you will discover **seventeen comprehensive unit tests**. They appear deceptively simple — and they absolutely are. But the **discipline they enforce** is critically important. Every rule we discussed above has a corresponding test, and every test executes in under a single millisecond WITHOUT touching the network, filesystem, or any external LLM:

```rust
#[test]
fn test_multiple_in_progress_rejected() {
    let mut mgr = TodoManager::new();
    let items = vec![
        serde_json::json!({"id": "1", "text": "A", "status": "in_progress"}),
        serde_json::json!({"id": "2", "text": "B", "status": "in_progress"}),
    ];
    let result = mgr.update(&items);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Only one task can be in_progress"));
}
```

### Two Types of Tests for Agent Systems

Agent harness systems require **two fundamentally different categories** of tests:

1. **Fast, purely deterministic tests** targeting the pure logic pieces: state validators, path sanitizers, message truncators, and renderers. These run in the continuous integration pipeline on **every single commit** and catch approximately **80% of all regressions**.

2. **Slow, flaky, and genuinely expensive tests** covering the fully end-to-end agent behavior: requiring a real LLM, real tasks, and a complete scoring methodology. These can realistically only run **nightly at best**.

### The Crucial Skill

The genuinely critical skill is making absolutely certain that the **pure pieces are genuinely pure**, so that the fast tests cover as much of the critical surface area as realistically possible. A `TodoManager` whose `update` method needed network access would be an absolute nightmare to test effectively. A `TodoManager` whose `update` is a **pure function** of `(old_state, new_items)` is utterly trivial to test. **When designing agent state, ask at every single step: "Can this be tested without any network call?"** If the honest answer is no, immediately separate the impure part into its own abstraction.

---

## Chapter 2 Summary and Transition

In this chapter, we have thoroughly examined some of the most important concepts in all of agent engineering:

1. **Established the fundamental problem** of why models "forget" and how plan drift destroys agent reliability over time.

2. **Designed the TodoManager structure** with explicit rationale for every field and constant — understanding why simple is always better than complex.

3. **Implemented critical invariants** that enforce good behavior without being overly restrictive — the art of choosing what to permit versus what to forbid.

4. **Justified the full-replace update semantics** rather than incremental diffs — understanding the deep reasons why simpler is more reliable.

5. **Designed the rendering format** — appreciating how seemingly minor presentation choices create massive downstream effects on model behavior.

6. **Established a clear taxonomy** of where different kinds of state belong in an agent system.

7. **Understood the testing philosophy** that makes this code reliable: fast deterministic tests for pure logic, accepting that some things truly require slow end-to-end validation.

In the **next chapter**, we will observe `TodoManager` from the other side of the equation. We will sit **inside the agent loop** in `src/agent_loop.rs` and see precisely how the harness transforms the model's abstract tool calls into actual tool results, round after round, until the conversation naturally concludes.

---

**Next:** Chapter 3 — The Agent Loop and Tool Results