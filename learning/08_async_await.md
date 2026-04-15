# Chapter 8: Prompt Engineering in Code

> "Text is the universal interface." — anonymous Unix aphorism, older than agents

Most of this book is about code. This chapter is about the
strings that code produces — system prompts, tool descriptions,
error messages, nag reminders — and about treating those strings
with the same rigour as any other interface in your system. An
agent's behaviour is shaped as much by its prompts as by its
Python or Rust, and the prompts live in version control right
next to the rest of the codebase.

We are not going to do "prompt engineering" in the folk sense.
No clever incantations, no "let's think step by step," no
whispering to the model. We are going to look at four specific
pieces of text in `rust_toy_agent`, understand why each one is
shaped the way it is, and derive patterns you can reuse.

The pieces:

1. The main agent's **system prompt**.
2. Individual **tool descriptions**.
3. The **nag reminder** from Chapter 3.
4. **Error messages** from the tool runners.

## 8.1 The System Prompt

Here is the system prompt for `rust_toy_agent`, as assembled in
`bin_core`:

```
You are a coding agent at {workdir}. Use the todo tool to plan
multi-step tasks. Mark in_progress before starting, completed
when done. Prefer tools over prose.
```

Three sentences. Forty-two words. That is the entire system
prompt. A lot of beginner agents ship with 500-word system
prompts full of bullet points, personas, escape clauses, and
step-by-step procedures, and they perform *worse* than this
one. Why?

**System prompts are read on every turn.** Every token in the
system prompt is a token the model re-reads, re-weights, and
has to integrate into its reasoning *every time you call it*. A
longer prompt isn't just more expensive — it is actively more
distracting. The model starts hedging against rules it hasn't
been asked to follow, quoting the prompt back at itself, and
producing output that reads like a civil-service memo.

The rule of thumb: **the system prompt should contain only the
things that must be true on every single turn.** Everything
task-specific goes in the first user message. Everything tool-
specific goes in the tool description.

Look at what this system prompt actually contains:

1. **Role** — "You are a coding agent at `{workdir}`." One
   sentence, places the model geographically and functionally.
   Note the interpolated workdir: the model always knows where
   it is. A model that does not know its working directory
   writes absolute paths to random places.

2. **A pointer to the todo tool** — "Use the todo tool to plan
   multi-step tasks." This is the only way the model learns
   that the todo tool *exists as a planning mechanism*. You
   cannot put this in the tool description — the tool
   description says *how* the tool works, not *when* to use
   it. Cross-tool coordination belongs in the system prompt.

3. **A behavioural rule** — "Mark in_progress before starting,
   completed when done." This teaches the workflow: transition
   the todo state in lockstep with the actual work. Without
   this line, the model will either never touch the todo list
   or update it in batches at the end.

4. **A preference** — "Prefer tools over prose." One of the
   most common failure modes of coding agents is that they
   explain what they are going to do instead of doing it.
   "Prefer tools over prose" is a short, learned incantation
   that consistently reduces this. Four words, measurable
   effect.

Notice what is *not* here:

* **No persona.** "You are a helpful assistant named Claude
  who..." — costs tokens, adds nothing.
* **No restating of API rules.** "Your output must be valid
  JSON" — the API enforces this already.
* **No exhaustive tool list.** The tool definitions are passed
  separately; duplicating them in prose is a waste.
* **No safety incantations.** "Do not run dangerous commands,
  do not edit files outside the workdir..." — the harness
  enforces this, and asking the model to also enforce it just
  gives you a chatty model that explains why it can't do
  things.

System prompts are cost centres. Keep them minimal, keep them
stable (remember prompt caching from Chapter 6), and put
task-specific context in the first user message where it
belongs.

## 8.2 Tool Descriptions: The Other System Prompt

Every tool carries its own slice of prompt. Tool descriptions
are in some ways more important than the system prompt, because
the model reads them with intent — it is actively trying to
pick a tool, and each description is a sales pitch.

`rust_toy_agent`'s descriptions are almost embarrassingly short:

```
bash:        "Run a shell command."
read_file:   "Read file contents."
write_file:  "Write content to file."
edit_file:   "Replace exact text in file."
todo:        "Update task list. Track progress on multi-step tasks."
```

One sentence each. No examples. No edge cases. No warnings.

Why does this work? Because the model already knows what a
shell command is, what reading a file means, what writing a
file means. A three-paragraph description of `bash` would not
teach the model anything it doesn't already know — and it would
compete for attention with the descriptions the model actually
*needs* to read.

When do you need longer descriptions? Two situations:

1. **The tool is unusual.** `todo` gets two sentences because
   planning-as-a-tool is not something the model sees everywhere.
   The second sentence ("Track progress on multi-step tasks.")
   frames *when* the model should reach for it.

2. **The tool has a non-obvious contract.** If `edit_file`
   required the old_text to be unique in the file, the
   description would say so: `"Replace exact text in file.
   old_text must appear exactly once in the file, or the edit
   fails."` The model needs to know the precondition before it
   writes the call.

Two things tool descriptions should almost never contain:

* **Examples.** Tempting ("show the model what a good call
  looks like"), but examples cost tokens and the model learns
  the format from the input_schema anyway. Save examples for
  when the model consistently calls the tool wrong.
* **Warnings about dangerous usage.** "Do not use this to
  delete system files" — if the tool can delete system files
  that is a *sandboxing* problem (Chapter 5), not a
  description problem. Telling the model "don't do X" makes
  the model think about X.

A good discipline: **write your tool descriptions, then cut them
in half**. Then cut them in half again if you can. The discipline
forces you to keep only what is load-bearing.

## 8.3 The Nag Reminder as Prompt

We met the nag reminder in Chapter 3. Here it is again:

```rust
let updated = format!("{content}\n\n<reminder>Update your todos.</reminder>");
```

Four words inside a custom tag, appended to a tool result. That
is prompt engineering in its purest form — a targeted string
that steers model behaviour — and it is embedded in the middle
of the harness's hot path, not in a config file.

Three things make the nag work:

1. **It arrives in a channel the model trusts.** Tool results
   are the most-read part of an agent's context. A system-prompt
   reminder would be re-read only if the model happened to go
   back to the system message, which it rarely does.

2. **The tag is distinctive.** `<reminder>` is a string the
   model does not produce itself, so when the model sees it in
   a tool result it knows the harness put it there. This
   matters because the model is less suspicious of content it
   produced than of content someone else did — inverted reality
   for once works in your favour.

3. **The message is imperative and specific.** "Update your
   todos" tells the model exactly what action to take. An
   abstract reminder like "remember to plan carefully" would
   produce vague planning-flavoured prose, not an actual tool
   call.

You will build other nag variants as you ship. Some you may
want:

* **"You have been reading files for several rounds without
  writing. Is there a plan forming?"** — catches the
  exploration-paralysis failure mode.
* **"You have called the same tool with the same arguments
  twice. If you are stuck, consider a different approach."** —
  catches the loop-on-failure failure mode.
* **"This session has run for 50 rounds. The user may expect
  a summary."** — catches the forever-running failure mode.

Each of these is five to fifteen tokens, injected into a
tool_result on a narrow trigger condition. Each one changes
agent behaviour more than a paragraph-long system prompt
rewrite. The format is: *detect the anti-pattern in harness
state, inject the smallest possible reminder into the next
tool_result*. That is the whole technique.

## 8.4 Error Messages Are Prompts Too

One of the under-appreciated surfaces in an agent harness is
the **error string**. When the model calls a tool and it fails,
the error message is what the model sees and reasons over. Bad
error messages cause bad agent behaviour; good ones fix it for
free.

Compare two versions of the same error. The model passes a
path that escapes the sandbox.

Bad:

```
Error: permission denied
```

Good (what `rust_toy_agent` does):

```
Error: path escapes sandbox: ../../etc/passwd
```

The bad version tells the model nothing useful. The model will
probably try again with a slightly different path, get the
same message, and fail in a loop. The good version tells the
model *what* went wrong (not a permission issue — a sandbox
issue) and *which* path it was (so the model can reason about
how to fix it). The model will usually correct itself on the
next turn.

The principles:

1. **Name the class of error.** `"Max 20 todos allowed"`,
   `"text required"`, `"invalid status"` — each one tells the
   model exactly which rule it broke.
2. **Include the offending input.** `"path escapes sandbox:
   ../../etc/passwd"` — the model can see, verbatim, what it
   sent, which is huge for recovery.
3. **Avoid anthropomorphic phrasing.** "I cannot let you do
   that" reads as a moral refusal, and the model argues with
   moral refusals. "path escapes sandbox" reads as a factual
   constraint, and the model respects factual constraints.
4. **Keep them short.** One line. The model reads error
   messages quickly; long errors get glossed over.

Read through `tool_runners.rs` and count the error messages.
Every one has been tuned. `"Error: Text not found in {path}"`,
`"Error: mkdir {dir}: {reason}"`, `"Error: Dangerous command
blocked"`. Each one is a one-line teaching moment, and
together they add up to a surprising amount of steering.

## 8.5 Where Prompt Text Lives

Every string in this chapter lives in a *code file*, not in a
YAML or JSON config. That is a considered choice. Three reasons:

1. **Prompt strings interact with code.** The system prompt
   interpolates the workdir. The nag reminder depends on round
   state. Error messages include runtime values. Hoisting the
   strings into a config file forces you to invent a templating
   system, and templating systems are where prompt bugs go to
   hide.

2. **Prompt changes need tests.** Changing a system prompt
   changes agent behaviour. Changing agent behaviour changes
   test outcomes. If the prompt lives next to the tests, a
   PR that touches the prompt also updates the fixtures, and
   the review catches regressions. If the prompt lives in a
   distant config, nobody notices it changed until the model
   starts misbehaving in production.

3. **Prompts are versioned.** Git history shows you every
   prompt change with a commit message and a diff. When an
   agent starts behaving badly "this week," `git log` on the
   prompt file is often your first and best debugger.

The one exception: **long, reusable prompt fragments** — say,
a 2000-word "tone and style" document you share across several
agents. Those can live in a `prompts/` directory as text files
that the code loads at startup. But keep the short, dynamic,
hot-path strings in code. They are code.

## 8.6 A/B Testing Prompts

You cannot improve a prompt without measuring. Every change to
a system prompt, every new tool description, every variation
on the nag reminder — each needs to be evaluated against a
benchmark, or you are just rearranging words and hoping.

The minimum viable prompt-evaluation setup:

1. **A set of tasks** with known correct outcomes. Not perfect
   outcomes — just "did the agent finish," "did it use the
   right tools," "did it update the todo list." A dozen tasks
   is enough to start, a hundred is enough to ship.

2. **A scoring function** that runs each task and emits a
   number. Token usage, round count, success/failure, tool-call
   distribution. Don't trust your gut — trust the table.

3. **A way to run both prompts against the same tasks.** Seed
   your RNG, hold the task input constant, vary only the prompt,
   run each combination ten times (because models are
   non-deterministic). Compare distributions, not single runs.

Modern coding agents have entire benchmark suites — SWE-Bench,
HumanEval, LiveCodeBench — that you can run your agent against.
Anything from a single-turn unit test to a week-long evaluation
counts. The point is to *have* the measurement, then change the
prompt, then run it again.

`rust_toy_agent`'s SWE-bench harness in `scripts/` is a rough
version of this. It is not the production evaluation you would
build for a real agent, but it is the shape: standardised
tasks, automated scoring, comparable runs.

## 8.7 Exercises

1. Rewrite the system prompt in three different ways — shorter,
   longer, and "same length but different structure." Predict
   which version performs best. Can you test your prediction
   with a benchmark?

2. Find a tool whose description is one sentence. Write a
   three-sentence version. Now write a zero-sentence version
   (empty description, just name and schema). Which do you
   think performs best, and why?

3. Add a nag reminder for the case where the model has
   written to the same file twice in three rounds. What
   condition triggers it, what does the reminder say, and
   where does it fire?

4. Audit `tool_runners.rs` for error messages. Find one that
   violates the principles in §8.4. Propose a better version
   and justify the change.

5. The workdir is interpolated into the system prompt. What
   else could you interpolate? The git branch? The number of
   uncommitted changes? The current time? For each, decide
   whether it belongs in the system prompt, the first user
   message, or nowhere at all.

In Chapter 9 we split the agent into parts — subagents with
fresh contexts, background tasks that run independently — and
ask when to decompose a task versus when to power through it
in one mind.
