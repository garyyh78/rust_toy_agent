# Chapter 8: Prompt Engineering in Code

> "Text is the universal interface." — anonymous Unix aphorism

## Introduction

Most of this book is about code. This chapter is about the strings that code produces — system prompts, tool descriptions, error messages, nag reminders — and treating those strings with the same rigour as any other interface.

We look at four specific pieces in `rust_toy_agent`:

1. The main agent's **system prompt**
2. Individual **tool descriptions**
3. The **nag reminder** from Chapter 3
4. **Error messages** from tool runners

## 8.1 The System Prompt

```
You are a coding agent at {workdir}. Use the todo tool to plan
multi-step tasks. Mark in_progress before starting, completed
when done. Prefer tools over prose.
```

Three sentences. Forty-two words. A lot of beginner agents ship with 500-word system prompts and perform *worse*.

**System prompts are read on every turn.** Every token is re-read, re-weighted. A longer prompt isn't just more expensive — it is actively more distracting.

**The rule of thumb:** system prompt should contain only things that must be true on every single turn. Everything task-specific goes in first user message. Everything tool-specific goes in tool description.

What this prompt contains:

1. **Role** — "You are a coding agent at `{workdir}`." Places model geographically and functionally. Note interpolated workdir: model always knows where it is.

2. **A pointer to the todo tool** — "Use the todo tool to plan multi-step tasks." Cannot put in tool description — tool description says *how* it works, not *when* to use it.

3. **A behavioural rule** — "Mark in_progress before starting, completed when done." Teaches workflow: transition todo state in lockstep with actual work.

4. **A preference** — "Prefer tools over prose." Most common failure mode is explaining what going to do instead of doing it.

What's NOT here:

- **No persona** — "You are a helpful assistant named Claude..." costs tokens, adds nothing
- **No restating of API rules** — API enforces this
- **No exhaustive tool list** — passed separately
- **No safety incantations** — harness enforces this

System prompts are cost centres. Keep minimal, keep stable.

## 8.2 Tool Descriptions: The Other System Prompt

`rust_toy_agent`'s descriptions:

```
bash:        "Run a shell command."
read_file:   "Read file contents."
write_file:  "Write content to file."
edit_file:   "Replace exact text in file."
todo:        "Update task list. Track progress on multi-step tasks."
```

One sentence each. No examples. No edge cases. No warnings.

Why does this work? Model already knows what a shell command is, what reading a file means. A three-paragraph description teaches nothing and competes for attention.

When you need longer descriptions:

1. **The tool is unusual** — `todo` gets two sentences because planning-as-a-tool isn't something model sees everywhere

2. **The tool has non-obvious contract** — if edit_file required old_text to be unique, description would say so

Tool descriptions should almost NEVER contain:

- **Examples** — model learns format from input_schema anyway
- **Warnings about dangerous usage** — sandboxing problem, not description problem

**Discipline:** write tool descriptions, then cut them in half. Then cut in half again.

## 8.3 The Nag Reminder as Prompt

From Chapter 3:

```rust
let updated = format!("{content}\n\n<reminder>Update your todos.</reminder>");
```

Four words inside custom tag, appended to tool result. Prompt engineering in purest form.

Three things make the nag work:

1. **Arrives in channel model trusts** — tool_results are most-read part of context. System-prompt reminder would be re-read only if model happened to go back.

2. **Tag is distinctive** — `<reminder>` is string model doesn't produce. When model sees it in tool_result, knows harness put it there.

3. **Message is imperative and specific** — "Update your todos" tells exactly what action to take.

Other nag variants:

- **"You have been reading files for several rounds without writing. Is there a plan forming?"** — catches exploration-paralysis
- **"You have called the same tool with same arguments twice. If stuck, consider different approach."** — catches loop-on-failure

Format: *detect anti-pattern in harness state, inject smallest possible reminder into next tool_result*.

## 8.4 Error Messages Are Prompts Too

When model calls a tool and it fails, error message is what model sees and reasons over. Bad error messages cause bad agent behavior.

Compare two versions. Model passes path that escapes sandbox:

**Bad:**
```
Error: permission denied
```

**Good (what rust_toy_agent does):**
```
Error: path escapes sandbox: ../../etc/passwd
```

Bad version tells model nothing useful. Model probably tries again with slightly different path, gets same message, fails in loop. Good version tells *what* went wrong and *which* path — model usually corrects itself next turn.

Principles:

1. **Name the class of error** — `"Max 20 todos allowed"`, `"text required"`, `"invalid status"`
2. **Include the offending input** — `"path escapes sandbox: ../../etc/passwd"`
3. **Avoid anthropomorphic phrasing** — "I cannot let you do that" reads as moral refusal; model argues with moral refusals. "path escapes sandbox" reads as factual constraint.
4. **Keep short** — one line. Model reads error messages quickly.

Read through `tool_runners.rs` and count error messages. Each one is a one-line teaching moment.

## 8.5 Where Prompt Text Lives

Every string in this chapter lives in a code file, not a YAML or JSON config. Three reasons:

1. **Prompt strings interact with code** — system prompt interpolates workdir. Nag reminder depends on round state. Hoisting into config forces inventing templating system.

2. **Prompt changes need tests** — changing prompt changes behavior. If prompt lives next to tests, PR that touches prompt also updates fixtures.

3. **Prompts are versioned** — git history shows every prompt change with commit message. When agent starts behaving badly, `git log` on prompt file is first debugger.

One exception: **long, reusable prompt fragments** — 2000-word tone-and-style document shared across agents. Those can live in `prompts/` directory as text files. But keep short, dynamic, hot-path strings in code.

## 8.6 A/B Testing Prompts

Cannot improve prompt without measuring. Every change needs evaluation against a benchmark.

Minimum viable setup:

1. **A set of tasks** with known correct outcomes — not perfect, just "did agent finish," "did it use right tools," "did it update todo list." A dozen is enough to start.

2. **A scoring function** — token usage, round count, success/failure, tool-call distribution. Trust the table, not gut.

3. **A way to run both prompts against same tasks** — seed RNG, hold task input constant, vary only prompt, run each combination ten times (because models are non-deterministic).

---

**Next:** Chapter 9 — Subagents and Background Work