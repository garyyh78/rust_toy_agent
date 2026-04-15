# Chapter 2: Working Memory — Giving the Agent a Scratchpad

> "Controlling complexity is the essence of computer programming." — Brian Kernighan

A large language model is, for our purposes, a stateless function:
you give it a prompt, it gives you a response, and the next call
remembers nothing unless you paste the previous turn into the input.
Everything we call "agent memory" is an illusion the harness creates
on the outside by carefully deciding what to put into the next prompt.

This chapter is about the simplest and most undervalued piece of that
illusion: a **todo list the model can read and write itself**. It is
not a data structure lesson. It is a lesson about what an agent needs
in order to stay on task across dozens of tool calls, why free-form
chain-of-thought is not enough, and how a fifty-line module can change
the feel of your agent from "flailing" to "purposeful."

The code we read is `src/todo_manager.rs`. You already saw the type
in Chapter 1. Now we ask: why does it exist?

## 2.1 The Problem: Models Forget What They Said

Picture an agent working on a realistic task: *"Migrate the user
service from Postgres to SQLite and update the integration tests."*
A careful human breaks that into half a dozen substeps — find the
service, read the schema, port the migrations, stub out the SQL
dialect differences, rerun the tests, handle fallout — and checks
them off mentally as they go.

A model, left to its own devices, does something worse than forget:
it *improvises* a plan on every turn. Round one it says "first I'll
read the schema." Round five it has read four files, completed two
edits, and says "let me think about the plan" — and the plan it
produces is subtly different from the one on round one, because the
context window has filled up with tool output and the model is
reconstructing its own intent from evidence.

This failure mode has a name in the agent-engineering community:
**plan drift**. It is the single biggest reason long-running agents
give up, repeat themselves, or ship half-finished work. The fix is
not a smarter model. It is an **externalised plan**: a structured
list the model writes into and reads back from, on every round,
with the same ceremony it uses to call a tool.

That is what `TodoManager` is. It is the agent's scratchpad, owned
by the harness, surfaced through a tool.

## 2.2 The Shape of the Scratchpad

Here is the whole type:

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

Three fields per item, one vector per agent session, one constant
cap at twenty. Every design choice here is load-bearing:

1. **`id` is a string, not an auto-incrementing integer.** The
   model assigns IDs when it writes the list. Human-readable IDs
   like `"schema-port"` or `"1"` survive paraphrase and partial
   re-planning. An auto-increment would force the harness to diff
   lists, which is exactly the kind of bookkeeping we want the
   model to do for us.

2. **`status` is a string with exactly three values.** Not a
   boolean `done`, not a `Done | Todo | Doing` enum. The three
   values match a vocabulary every model already knows: *pending*,
   *in_progress*, *completed*. Adding a fourth state — say,
   *blocked* — is tempting, and wrong, until you have real data
   showing the model wants it. Agent vocabularies should be
   chosen once and then defended against creep.

3. **`MAX_TODO_ITEMS = 20`.** Twenty is not a magic number. It is
   a statement about attention: a plan longer than twenty items
   is almost certainly a plan the model will not finish, and is
   almost always a plan that should have been two plans. The cap
   forces the model to decompose hard problems into subtasks with
   their own plans — usually via subagents, which we meet in
   Chapter 9.

4. **There is no nesting, no priority, no assignee, no deadline.**
   A todo list for a human project manager has all of those. An
   agent todo list does not, because the model is the only worker
   and the harness is the only scheduler. Every field that exists
   must pay rent: it must be something the model will read, write,
   or reason about on a meaningful fraction of turns. Fields that
   sound useful but go unused are just tokens the model has to
   scan past on every round.

## 2.3 The Invariants

Read the validation loop again, from `update`:

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

Four rules, each fighting a specific failure mode of real agents:

* **Cap the size.** Stops the model from turning the todo list into
  a brain dump and burning tokens on it every round.
* **Reject empty text.** Stops the model from creating placeholder
  rows like `{id: "2", text: "", status: "pending"}` and then
  filling them in later — an easy habit to fall into, and one that
  makes the rendered list unreadable.
* **Enum the status.** Stops the model from inventing states. If
  you let the model write `status: "doing"`, it will, and then on
  round seventeen it will write `"in-progress"`, and your renderer
  will silently miscount. An enum check with a useful error
  message teaches the model within one round.
* **Exactly one `in_progress`.** This is the important one. It
  enforces single-tasking. A well-behaved agent has exactly one
  task in flight and knows what it is. The moment you allow two,
  the model starts *plan-hopping* — jumping between half-finished
  tasks as it notices new subproblems — and the interaction turns
  into a tangle.

Notice what is not enforced: you can mark a `completed` task back
to `pending`, you can reorder items, you can delete half the list.
These are things humans do all the time during planning, and
forbidding them would force the model into awkward workarounds.
The rule of thumb for invariants on an agent-facing data structure
is: **enforce the things that break you, permit the things that
just look untidy.**

## 2.4 Update Semantics: Replace, Don't Diff

Look at the last lines of `update`:

```rust
self.items = validated;
Ok(self.render())
```

The new list completely replaces the old one. The model does not
send deltas like `{add: [...], remove: [...]}`, and it does not
send per-item patches. It sends the entire list it wants the
state to be, every time.

This is a deliberate and important choice. Three reasons:

1. **Models are great at writing lists and terrible at diffing.**
   Asking the model to produce a JSON patch against a list it
   remembers from three rounds ago is a recipe for subtle bugs.
   Asking it to produce the full list it wants is something every
   model does reliably from its first day of training.

2. **A full-replace tool is idempotent.** If the network flakes
   and the tool call runs twice, the state is the same. Agents
   should aim for idempotent tools wherever possible; Chapter 5
   leans on this principle again.

3. **The render of the new state is returned as the tool result.**
   The model sees exactly what the harness now believes, and the
   next round's context contains the canonical rendering rather
   than the model's own memory of what it meant to write. This
   is a small anti-drift trick that punches way above its weight.

The cost is that the model has to retype the whole list each round
it changes anything. In practice this costs perhaps fifty tokens
per update and buys a completely reliable scratchpad. Worth it.

## 2.5 The Render: The Format Matters

The model does not see `Vec<TodoItem>`. It sees whatever string the
harness renders:

```
[ ] #1: Write failing test
[>] #2: Port schema
[x] #3: Read existing migrations
(1/3 completed)
```

This is the only part of `TodoManager` the model ever touches with
its eyes. Look at what it does and what it does not do:

* **Status glyphs are distinctive.** `[ ]`, `[>]`, `[x]` — three
  bracketed characters. They do not collide with anything the
  model is likely to see in file contents, and they scan
  vertically. Do *not* use emoji: they tokenize into multiple
  tokens, they render inconsistently across terminals, and they
  sometimes trigger the model to reply in emoji, which nobody wants.

* **The ID is prefixed with `#`.** Not because the harness needs
  the `#`, but because it makes the ID visually distinct from the
  text, and because when the model references `#3` in its
  reasoning it is clearly pointing at a todo item, not a heading
  or an issue number.

* **The progress line is absolute, not percentage.** `(1/3
  completed)` beats `(33%)` because the model reads it and knows
  both the remaining count and the total. Percentages hide the
  scale of the work.

* **Empty lists render as `No todos.` not as a blank string.**
  Blank strings confuse models. They wonder whether the tool
  ran, whether the previous state is still in effect, whether
  they need to create a list. A three-word message answers all
  of that.

These are tiny decisions. They are also the decisions that
separate an agent that uses the todo tool naturally from an agent
that needs to be cajoled into it. Spend the extra ten minutes on
the render; it is almost always the highest-leverage code in the
whole module.

## 2.6 What State Lives Here — and What Doesn't

The todo list is not the agent's only working memory. It is one
of several things the harness tracks on the side of the
conversation:

| state | lives in | lifetime | visible to model |
| --- | --- | --- | --- |
| conversation history | `Messages` vec | the session | yes, always |
| todo list | `TodoManager` | the session | on demand, via tool |
| working directory | `WorkdirRoot` | the session | implicit in tool results |
| background tasks | `BackgroundManager` | the session | on demand, via tool |
| session log | `SessionLogger` | the session | no (human-only) |

A useful rule for deciding where a new piece of state belongs:

> **If the model needs to read it every turn, put it in the system
> prompt. If the model needs to read it sometimes, expose it through
> a tool. If the model never needs to read it, keep it in the
> harness and log it for humans.**

`TodoManager` lives in the middle bucket. The list is relevant most
turns but not all; the model asks for it when planning and updates
it when status changes. Burning system-prompt tokens to show the
list on every call would be wasteful — worse, it would encourage
the model to *not* update the list, because the state would feel
"pushed" rather than "owned."

Tools are the natural home for pull-based state.

## 2.7 Testing the Scratchpad

Scroll to the bottom of `todo_manager.rs` and you will find
seventeen unit tests. They look simple — and they are. But the
discipline they enforce is important. Every rule we discussed above
has a test, and every test runs in under a millisecond without
touching the network, the filesystem, or an LLM:

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

Agent harnesses are systems with two kinds of tests:

1. **Fast, deterministic tests** around the pure pieces: state
   validators, path sanitisers, message truncators, renderers.
   These run in CI on every commit and catch 80% of regressions.

2. **Slow, flaky, expensive tests** around the end-to-end agent:
   a real LLM, a real task, a real scoreboard. These run nightly
   at best.

The skill is making sure the pure pieces are genuinely pure, so
the fast tests cover as much of the surface as possible. A
`TodoManager` whose `update` needed network access would be a
nightmare to test. One whose `update` is a pure function of
`(old state, new items)` is trivial. When you design agent
state, ask at every step: *can this be tested without a network
call?* If the answer is no, separate the impure part.

## 2.8 Exercises

1. Add a `cancelled` status to `TodoManager` and see how many
   other files in `src/` mention the existing statuses. How
   many places would need to know about `cancelled`? What does
   that tell you about the cost of adding a status?

2. The current render sorts items by their position in the
   input array. Would you sort `in_progress` first? Why might
   the model behave differently if you did?

3. Replace the hard cap of 20 items with a soft warning: allow
   up to 40 items but prepend `⚠ Long plan — consider
   subagents` to the render when there are more than 20.
   Would this change the model's behaviour? How would you
   measure it?

4. Write a `TodoManager::stats()` method that returns a
   `(pending, in_progress, completed)` triple. Then decide:
   should this be exposed to the model as a new tool, or kept
   as a harness-only helper? Re-read §2.6 before answering.

5. Invent a second piece of harness state — say, "files the
   agent has already read" — and sketch its API. Does it
   belong in the system prompt, behind a tool, or in the
   harness only? Defend your choice.

In Chapter 3 we watch `TodoManager` from the other side: we sit
inside the agent loop in `src/agent_loop.rs` and see how the
harness turns the model's tool calls into tool results, round
after round, until the conversation naturally ends.
