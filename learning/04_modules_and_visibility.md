# Chapter 4: Tool Design — The Contract With the Model

> "The best programs are written so that computing machines can perform them quickly and so that human beings can understand them clearly." — Donald Knuth

Tools are the agent's hands. Whatever the harness chooses to expose —
and whatever it chooses to *not* expose — defines the agent's
capabilities, its failure modes, and, quietly, its personality. This
chapter is about the craft of picking tools and shaping them so the
model uses them well.

The code we read is `src/tools.rs` (the catalog and dispatcher) and
`src/tool_runners.rs` (the implementations). Together they are about
four hundred lines. Every line is a decision.

## 4.1 What Counts as a Tool

In the tool-use protocol, a tool is four things:

1. A **name** — a short string the model sees and repeats.
2. A **description** — one or two sentences the model reads to
   decide when to call it.
3. An **input schema** — JSON Schema describing the arguments.
4. A **runner** — the harness code that takes the arguments and
   produces a string result.

The JSON blob for the `bash` tool in `rust_toy_agent`:

```json
{
  "name": "bash",
  "description": "Run a shell command.",
  "input_schema": {
    "type": "object",
    "properties": {"command": {"type": "string"}},
    "required": ["command"]
  }
}
```

Four properties, ten lines, zero wasted words. The description is
four words. That is not laziness; it is calibration. `bash` is the
most famous tool in software, and the model already knows what a
shell command is. A paragraph of explanation here would dilute the
model's attention for the tools it *doesn't* already know.

The opposite of this is the `todo` tool, whose description is
`"Update task list. Track progress on multi-step tasks."` — two
sentences, because the concept needs teaching. **Description length
should scale with how surprising the tool is**, not with how
important it is.

## 4.2 The Five Tools

`rust_toy_agent` exposes exactly five tools to the main agent:

| tool | input | output | purpose |
| --- | --- | --- | --- |
| `bash` | `command` | stdout + stderr, capped | the universal escape hatch |
| `read_file` | `path`, `limit?` | file contents, capped | inspect code |
| `write_file` | `path`, `content` | confirmation | create new files |
| `edit_file` | `path`, `old_text`, `new_text` | confirmation | modify existing files |
| `todo` | `items[]` | rendered list | externalised planning |

Five. Not fifteen. The temptation to add more tools — `grep`,
`find`, `git_status`, `list_directory`, `count_lines`, `move_file`,
`search_and_replace` — is constant, and you must resist it. Every
tool you add:

* **Expands the system prompt** the model must read before every
  response, costing tokens on every round.
* **Adds a new failure mode** the harness must test and debug.
* **Competes for the model's attention** — two tools that do
  similar things force the model to make an irrelevant choice
  and sometimes make the wrong one.

The `bash` tool eats the entire category of "things I could do with
a shell command." `find`, `grep`, `ls`, `wc`, `git status` — all of
them are one-liners through `bash`, and the model already knows the
syntax. Adding a dedicated `grep` tool is usually a regression, not
an improvement.

**The rule of thumb: add a tool only when `bash` is insufficient or
actively dangerous.**

Why does `read_file` exist, then? Two reasons. First, the harness
truncates and line-counts the output in ways a naive `cat` would
not. Second, and more importantly, `read_file` is *idempotent and
safe by construction* — no shell interpretation, no globs, no
redirection. The model can use it without thinking about quoting.
That matters because read operations dominate an agent's tool use,
and you want the common path to be the easy one.

`write_file` and `edit_file` exist for the same reason: writing a
file through `bash` requires heredocs or `printf` with escaped
newlines, and the model gets them wrong often enough that a
dedicated tool pays for itself in one session.

## 4.3 The `edit_file` Decision

Of the four file tools, `edit_file` deserves a section of its own.
Its input schema:

```json
"required": ["path", "old_text", "new_text"]
```

The tool replaces the first occurrence of `old_text` in `path` with
`new_text`. No regex, no line numbers, no patch format. Just a
triple of literal strings.

This is the format every modern coding agent has converged on, and
it is worth understanding why the alternatives lost:

* **Line numbers are a nightmare.** Models do not count well, and
  line numbers shift between turns as edits land. A "replace lines
  42-57" tool misfires constantly.
* **Regex patches invite injection bugs.** The model writes a
  pattern, the harness compiles it, the pattern has an unescaped
  special character, and suddenly the edit is a no-op or matches
  too much. Worse, the errors are silent.
* **Unified diffs are too token-heavy.** A six-byte change becomes
  a twenty-line diff blob, and the model has to generate the
  context lines exactly right, which it does not always manage.
* **Direct AST edits are language-specific.** They work for one
  codebase and break for the next. A coding agent must not care
  whether the file is Python or TOML.

The `old_text` / `new_text` format sidesteps all of this. The model
quotes enough surrounding context to make `old_text` unique in the
file, the harness does a literal `str::replacen(old_text, new_text,
1)`, and the result is either a clean change or a clean failure
("old_text not found" / "old_text matched more than once"). Both
outcomes are things the model can recover from on the next turn.

The one gotcha: if `old_text` is not unique, the tool replaces the
*first* occurrence. In `rust_toy_agent`'s implementation it is
`replacen(_, _, 1)` — one replacement. A more defensive version
would error out and demand more context, and most modern agents do.
This is worth noting as you design your own edit tool: **failing
loudly is almost always better than doing something plausible**.

## 4.4 Shaping Inputs: What Makes a Good Schema

Look at `read_file`:

```json
"properties": {
  "path": {"type": "string"},
  "limit": {"type": "integer"}
},
"required": ["path"]
```

One required field, one optional. Observations:

1. **`path` is a string, not an array of strings.** The model
   *wants* to batch sometimes — "read these five files" — and
   you could support it. But the cost is an output format that
   combines five file contents into one blob, and now the model
   has to parse your delimiters. Keep tools single-purpose and
   let the model make N calls. The protocol supports multiple
   tool_use blocks per round (see Chapter 3), so this is cheap.

2. **`limit` is optional, not defaulted.** The model decides when
   it wants a bounded read. If you default to, say, 500 lines,
   you make two wrong choices at once: a ceiling the model can't
   raise, and a floor that wastes tokens for small files.
   Optional-with-no-default is almost always the right pattern.

3. **There is no `offset`.** We do not support pagination. The
   harness caps output at 50 KB and that is the end of the
   conversation — if the file is bigger, the model can use
   `bash` with `sed -n`. Pagination sounds helpful but adds a
   stateful two-step that the model often gets wrong; we let
   `bash` carry that weight.

The north star when shaping an input schema: **every argument
should have a clear, non-overlapping job, and every optional
argument should have a reason to be optional**. If you find
yourself writing a tool with six optional fields, you probably
have two or three tools fused together. Split them.

## 4.5 Shaping Outputs: What the Model Sees

A tool's output is a string — the `content` of a `tool_result`
block. What goes in that string is one of the most consequential
design choices in agent engineering.

`rust_toy_agent`'s `run_bash` returns something like:

```
$ git status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean

(exit 0)
```

Three features worth copying:

1. **The command is echoed at the top.** The model sees `$ git
   status` and knows exactly which call produced which output —
   even if it dispatched three `bash` calls in one round.
2. **Stdout and stderr are interleaved** (or clearly labelled if
   separated). Many tools write status to stderr and content to
   stdout, and a model that only sees stdout will be baffled by
   `ssh`'s "Are you sure you want to continue connecting?" prompt
   that lives on stderr.
3. **The exit code is explicit.** `(exit 0)` or `(exit 1)` or
   `(timeout)`. Do not rely on the model guessing from the text
   whether the command succeeded. It will guess wrong in the
   interesting cases, which are exactly the ones where you need
   it to be right.

Outputs from `read_file` follow the same spirit:

```
src/agent_loop.rs (1-200 of 400 lines, 12_345 bytes):
   1  //! agent_loop.rs
   2  ...
```

Line numbers on each line, the range in the header, and a byte
count for situations where line counts lie (binary files, very
long single-line files). This is the same output format every
modern coding agent uses, give or take some decoration, because
models reference files by line number in their reasoning and they
need the numbers to match.

One more rule: **truncate, and say you truncated.** `rust_toy_agent`
caps tool output at `MAX_TOOL_OUTPUT_BYTES` (50 KB). When it hits
the cap, the output ends with:

```
... (output truncated, 1_234_567 bytes total)
```

The model needs to know *that* it was truncated and *how much*
was dropped. Without the notice it will happily plan its next
move based on an incomplete view. With the notice, it narrows its
search on the next turn — usually by adding a `grep` or a `head`.

## 4.6 Dispatch: The Router

Here is the body of `dispatch_tools`:

```rust
pub fn dispatch_tools(
    name: &str,
    input: &Json,
    workdir: &WorkdirRoot,
    todo: &Mutex<TodoManager>,
) -> (Option<String>, bool) {
    match name {
        "bash" | "read_file" | "write_file" | "edit_file" => {
            (dispatch_basic_file_tool(name, input, workdir), false)
        }
        "todo" => {
            let items = input["items"].as_array().cloned().unwrap_or_default();
            let mut mgr = todo.lock().unwrap();
            let result = mgr.update(&items).unwrap_or_else(|e| format!("Error: {e}"));
            (Some(result), true)
        }
        _ => (None, false),
    }
}
```

Two important things:

1. **The dispatcher returns an `Option<String>` and a `bool`.** The
   bool is the "was this the todo tool" flag we met in Chapter 3,
   used by the nag logic. The option distinguishes "unknown tool"
   from "known tool that produced empty output." **Do not conflate
   these two cases.** An unknown-tool response should tell the
   model the tool does not exist; an empty-output response should
   say `(no output)` or similar.

2. **Unknown tools do not panic.** If the model invents a tool
   name — which it does, especially under stress — the harness
   returns `Unknown tool: {name}` and the loop continues. The
   model sees the error and typically picks the right tool on
   the next turn. A panic would end the session with a
   stack trace the model can't read.

The dispatcher is deliberately small. Every tool gets a `match`
arm and a one-line call. If you find yourself writing complex
logic inside the dispatcher, you are conflating routing with
running — split them, and let the dispatcher do one thing.

## 4.7 The Parent/Child Tool Split

Look near the bottom of `tools.rs`:

```rust
pub fn child_agent_tools() -> Vec<Json> {
    vec![tool_bash(), tool_read_file(), tool_write_file(), tool_edit_file()]
}

pub fn parent_agent_tools() -> Vec<Json> {
    // same four + tool_todo() + tool_subagent() + ...
}
```

The parent agent and its subagents see *different* tool sets.
Subagents get the basic file tools. Only the parent gets `todo`,
`subagent` (to spawn further children), and the background-task
tools. Why?

* **Subagents have short lifetimes** (capped at 30 turns, usually
  shorter). Planning tools are overkill for one-shot tasks.
* **Only one level of subagents** keeps the tree manageable. If
  every subagent could spawn more subagents, you get fan-out
  explosions and runaway API bills.
* **Fewer tools, faster responses.** The tool list goes into every
  prompt. Removing the three or four tools a subagent does not
  need shaves dozens of tokens off every call.

Tool sets are part of the agent's identity. A parent and a child
are not two instances of the same agent; they are two different
agents with overlapping toolboxes. Think about tool scoping
deliberately — it is a free way to give the model a clearer idea
of what role it is playing right now.

## 4.8 The Runners: Safety First

Each tool's runner lives in `tool_runners.rs`. They all begin
with the same incantation:

```rust
pub fn run_read(path: &str, limit: Option<usize>, workdir: &WorkdirRoot)
    -> String
{
    let resolved = match safe_path(path, workdir) {
        Ok(p) => p,
        Err(e) => return format!("Error: {e}"),
    };
    // ... actually read the file ...
}
```

`safe_path` is the sandbox: it takes a user-supplied path, joins
it to the workdir, canonicalizes it, and refuses anything that
escapes the workdir root. We spend all of Chapter 5 on how this
works and why it is necessary. The thing to note now is the
*shape*: every runner starts by resolving paths through
`safe_path`, and every failure returns a string error rather than
panicking or crashing.

The single hardest lesson in agent tool design is this:

> **A tool runner must never panic, never hang indefinitely, and
> never modify state outside the workdir. Everything else is
> negotiable.**

Not "should not." Not "tries not to." Must not. Every panic is a
dead session, every hang is a dead harness process, and every
out-of-workdir write is a security bug. The five runners in
`rust_toy_agent` collectively enforce these three invariants, and
so does every production agent you will ever look at.

## 4.9 Exercises

1. Add a sixth tool, `list_directory`, with input `{path}` and
   output a flat listing. Before you implement it, write down
   three reasons why it might *not* be worth adding (hint:
   reread §4.2). Still want it? Write the schema.

2. The `bash` tool currently runs with `env_clear` and only
   allows a specific list of environment variables through.
   Open `tool_runners.rs` and find the allowlist. Would you
   add anything? Would you remove anything? Justify each.

3. `edit_file` replaces the first occurrence of `old_text`. Add
   a new variant, `edit_file_all`, that replaces every
   occurrence. Now argue the opposite case — that the new tool
   should not exist and the existing tool should instead error
   if `old_text` matches more than once. Which design makes
   more mistakes recoverable?

4. Sketch the JSON schema for a `git_diff` tool. Then decide
   whether to add it to the catalog or leave the job to `bash`.
   Write down your reasoning.

5. Read the subagent tool list (`child_agent_tools()`). Imagine
   adding a `web_search` tool. Would it go in the parent list,
   the child list, both, or neither? Why?

In Chapter 5 we follow the paranoid thread started in §4.8 and
look at sandboxing: path traversal, canonicalization, environment
allowlists, and the workdir root that anchors the whole system.
