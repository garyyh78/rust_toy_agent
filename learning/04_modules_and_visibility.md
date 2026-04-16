# Chapter 4: Tool Design — The Contract With the Model

> **"The best programs are written so that computing machines can perform them quickly and so that human beings can understand them clearly."** — *Donald Knuth*

---

## Introduction: Tools as the Agent's Hands

**Tools are the agent's hands.** Whatever the harness deliberately chooses to expose — and whatever it equally deliberately chooses to *not* expose — fundamentally defines three critical aspects of the agent: its genuine capabilities, its distinct failure modes, and perhaps most subtly, its emerging personality. This chapter is dedicated to the **craft of selecting tools** and **shaping them with care** so that the model uses them effectively and correctly every single time.

The code we will examine comes from two primary source files: `src/tools.rs` (which contains the tool catalog and the dispatcher) and `src/tool_runners.rs` (which contains all the implementations). Together, these two files encompass approximately **four hundred lines** of carefully considered code — and literally **every single line** represents a deliberate decision with real consequences.

---

## 4.1 What Precisely Counts as a Tool

In the tool-use protocol that defines how the agent communicates with the model, a tool is unambiguously defined as **exactly four distinct components**:

| Component | Description | Example |
|-----------|-------------|---------|
| **Name** | A short string that the model sees and repeats when invoking the tool | `"bash"`, `"read_file"`, `"todo"` |
| **Description** | One or two sentences that the model actively reads to determine when to call it | `"Run a shell command."` |
| **Input Schema** | A JSON Schema object comprehensively describing all acceptable arguments | `{"type": "object", "properties": {...}}` |
| **Runner** | The actual harness code that accepts arguments and produces a string result | The `run_bash` function in Rust |

### The JSON Tool Definition

The precise JSON blob defining the `bash` tool in `rust_toy_agent` looks like this:

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

This definition contains **four properties** and occupies **ten lines** with **zero wasted words**. The description is merely **four words long**. This is absolutely **not** laziness — it is careful **calibration**. The `bash` tool is the single most famous and widely understood tool in all of software engineering, and the model already comprehensively understands what a shell command is and does. A lengthy paragraph of explanatory text here would **dilute the model's precious attention** for the tools it genuinely does NOT already know.

### The Comparison with Todo Tool

The polar opposite approach is taken with the `todo` tool, whose description is deliberately more expansive:

```
"description": "Update task list. Track progress on multi-step tasks."
```

This is **two sentences**, because the concept genuinely requires some teaching — the model does not inherently understand how this particular scratchpad works or why it should update it regularly.

> **The Essential Calibration Rule:** Description length should scale precisely with how **surprising** or **unusual** the tool is, NOT necessarily with how **important** the tool may be.

---

## 4.2 The Exactly Five Tools

`rust_toy_agent` exposes **exactly five deliberately chosen tools** to the main agent:

| Tool Name | Input | Output | Purpose |
|----------|-------|--------|---------|
| **`bash`** | `command` (string) | stdout + stderr, intelligently capped | The universal escape hatch for any operation |
| **`read_file`** | `path` (string), `limit?` (optional integer) | File contents, capped | Inspect code and text files |
| **`write_file`** | `path` (string), `content` (string) | Confirmation message | Create brand new files |
| **`edit_file`** | `path`, `old_text`, `new_text` | Confirmation or error message | Modify existing files with precision |
| **`todo`** | `items[]` (array) | Rendered list with progress | Externalized planning and state tracking |

### The Critical Discipline: Five, Not Fifteen

**Five tools. Not fifteen.** The persistent temptation to add more tools — such as `grep`, `find`, `git_status`, `list_directory`, `count_lines`, `move_file`, `search_and_replace` — is constant and must be **actively resisted**. Here is exactly why adding tools is genuinely harmful:

| Problem | Consequence |
|--------|------------|
| **System prompt expansion** | Every tool added expands the system prompt the model must read before EVERY response, costing precious tokens on EVERY single round |
| **New failure modes** | Every new tool adds an entirely new category of failure modes that the harness must comprehensively test and debug |
| **Attention competition** | Two tools that perform similar operations force the model to make a frequently irrelevant choice, and sometimes it chooses WRONG |

### Why Bash Eats Everything

**The `bash` tool comprehensively consumes the entire category** of "things I could do with a shell command." The commands `find`, `grep`, `ls`, `wc`, `git status` — absolutely all of them are simple one-liners accessible through `bash`, and the model already knows the complete syntax. **Adding a dedicated `grep` tool is usually a regression, not an improvement** — it fragments the model's attention without adding meaningful capability.

> **Strict Rule of thumb:** Add a completely new tool ONLY when `bash` is genuinely **insufficient** for the task OR actively **dangerous** without proper isolation.

### Why read_file Exists Separately

So why does `read_file` exist as a distinct tool when `bash` can run `cat`? Precisely **two compelling reasons**:

1. **The harness intelligently truncates and manages line counts** in ways that a naive `cat` command would not — providing consistent, predictable output formatting.

2. **Critically, `read_file` is idempotent and safe by absolute construction**: absolutely NO shell interpretation, NO wildcard expansion, NO redirection possibilities. The model can use this tool **without thinking about quoting or escaping**. This matters enormously because **read operations dominate an agent's total tool usage**, and you absolutely want the most common execution path to be the easy, safe one.

### Why write_file and edit_file Exist

`write_file` and `edit_file` exist for the identical fundamental reason: writing a file through `bash` requires either heredocs or carefully escaped `printf` statements with properly escaped newlines, and the model gets these wrong **often enough** that a dedicated tool genuinely **pays for itself in a single session** of non-trivial editing.

---

## 4.3 The edit_file Decision: One of The Most Critical Tools

Of the **four file-related tools**, `edit_file` absolutely deserves a dedicated section of its own. Its precise input schema:

```json
"required": ["path", "old_text", "new_text"]
```

The tool replaces the **first occurrence** of `old_text` in the specified file path with `new_text`. There is absolutely **NO support for regex**, **NO line number dependencies**, and **NO complex patch format**. Just a straightforward triple of literal strings.

### The Design Convergence

This specific format — `old_text` / `new_text` as literal strings — is EXACTLY what **every modern coding agent has convergently arrived at**, and it is genuinely worth understanding in depth **why all the alternatives lost**:

### Why Line Numbers Are Terrible
> **Models do not count well.** Line numbers shift between turns as edits land. A tool promising to "replace lines 42-57" misfires constantly because the model cannot reliably count to 42 — or more accurately, it cannot reliably account for prior edits that have already shifted line numbers.

### Why Regex Patches Invite Catastrophic Bugs
> The model writes a regex pattern, the harness compiles it as-is, the pattern contains an unescaped special character character, and **the edit silently becomes a no-op OR matches far too much content than intended**. The truly terrifying part: these errors are **completely silent** — the tool reports "success" but the file is wrong.

### Why Unified Diffs Consume Too Many Tokens
> A six-byte change typically becomes a **twenty-line diff blob** — the model has to generate all the context lines **exactly accurately** for the diff to apply, and it does not always manage this correctly. The tool failure rate becomes unacceptable.

### Why Direct AST Edits Are Unscalable
> AST editing works beautifully for one specific codebase and **completely breaks** for the next. A genuine coding agent absolutely must not care whether the target file is Python or TOML or Rust or JavaScript.

### The Old_Text / New_Text Format Wins

The **`old_text` / `new_text` format** elegantly sidesteps absolutely ALL of these issues. The model quotes a generous enough surrounding context to make `old_text` **unique within the file**, the harness performs a literal `str::replacen(old_text, new_text, 1)` operation, and the result is **either a cleanly successful change OR a cleanly reported failure** ("old_text not found" / "old_text matched more than once"). **Both** of these outcomes are things the model can effectively recover from on the very next turn.

### The Critical Gotcha

The one genuine gotcha: if `old_text` is deliberately **not unique** within the file, the tool automatically replaces the **first occurrence only**. In `rust_toy_agent`'s implementation, this is explicit: `replacen(_, _, 1)` — precisely **one replacement**. A more defensive version would eagerly error out and demand more surrounding context from the model, and most modern agents absolutely do take this approach.

> **The Professional Insight:** Failing **loudly and clearly** is almost always **superior** to doing something that seems plausible but may be incorrect.

---

## 4.4 Shaping Inputs: What Makes an Exceptional Schema

Let us examine the `read_file` input schema very carefully:

```json
"properties": {
  "path": {"type": "string"},
  "limit": {"type": "integer"}
},
"required": ["path"]
```

### Three Essential Observations

1. **`path` is a single string, NOT an array of strings** (highlighted in **blue**). The model *wants* to batch operations sometimes — requesting "read these five files" is a natural pattern. However, the genuine cost is that you must now design an **output format** that combines five distinct file contents into a single blob, and now the model must parse your custom delimiters intelligently. **Keep tools single-purpose and let the model make N independent calls.** The protocol already supports multiple tool_use blocks per single round (see Chapter 3), so this approach is genuinely cheap.

2. **`limit` is optional with ABSOLUTELY NO default value** (highlighted in **green**). The model decides precisely when it wants a bounded read. If you default to, for example, 500 lines, you make **TWO wrong choices** at exactly the same time: a ceiling the model CANNOT raise AND a floor that wastes tokens for trivially small files. **Optional-with-no-default is almost always the correct pattern.**

3. **There is absolutely NO `offset` field** (highlighted in **purple**). We do NOT support pagination. The harness intelligently caps output at 50 KB total and considers that conversation complete — if the target file genuinely exceeds this limit, the model can always use `bash` with `sed -n` to perform its own pagination. Pagination **sounds helpful** but adds a new **stateful two-step** that the model frequently gets wrong; we deliberately let `bash` carry that weight instead.

> **The North Star Principle:** Every single argument in an input schema should have a clear, definitively non-overlapping job, and **every optional argument should have a specific, deliberate reason to be optional**. If you find yourself writing a tool with six or seven optional fields, you almost certainly have **two or three separate tools fused together** that should be deliberately split apart.

---

## 4.5 Shaping Outputs: What the Model Actually Sees

A tool's output is fundamentally a string — the `content` field of a `tool_result` block. **What goes into that string** is one of the most consequential design choices in the entirety of agent engineering.

### The run_bash Output Format

`rust_toy_agent`'s `run_bash` tool returns output formatted exactly like this:

```
$ git status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean

(exit 0)
```

This format contains **three essential features** that are absolutely worth copying faithfully:

| Feature | Why It Matters |
|---------|-------------|
| **The command is echoed at the top** | The model sees exactly `$ git status` and knows precisely which call produced which output — critical when the model dispatched three different `bash` calls in a single round |
| **Stdout and stderr are clearly interleaved** (or labelled if separated) | Many tools write status information to stderr and actual content to stdout. A model that only sees stdout will be completely baffled by `ssh`'s "Are you sure you want to continue connecting?" prompt that lives on stderr |
| **Exit code is absolutely explicit** | `(exit 0)` or `(exit 1)` or `(timeout)`. Never rely on the model guessing from the text whether the command succeeded — especially in the genuinely interesting cases where precision matters most |

### The read_file Output Format

`read_file` outputs follow the identical philosophical spirit:

```
src/agent_loop.rs (lines 1-200 of 400 total, 12,345 bytes):
   1  //! agent_loop.rs
   2  ...
```

This format includes **line numbers on literally every line**, the specific range in the header, and an actual **byte count** for situations where line counts are genuinely misleading (binary files, extremely long single-line files). **Every single modern coding agent uses this format**, give or take minor decoration, precisely because models reference files by line number in their reasoning and they absolutely need those numbers to match.

### The Mandatory Truncation Notice

**One more absolutely critical rule:** **Truncate, and explicitly SAY that you truncated.** `rust_toy_agent` caps tool output at `MAX_TOOL_OUTPUT_BYTES` (50 KB). When it hits this hard cap, the output ends with the explicit notice:

```
... (output truncated, 1,234,567 bytes total)
```

The model absolutely needs to know **THAT truncation occurred** and **HOW MUCH data was dropped** — without this explicit notice, it will happily plan its next move based on a fundamentally incomplete view. **With** the truncation notice, the model narrows its search on the next turn — almost always by adding a `grep` or a `head` command to be more specific.

---

## 4.6 The Dispatcher: The Router Mechanism

Here is the core body of the `dispatch_tools` function:

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

### Two Absolutely Essential Observations

1. **The dispatcher returns BOTH an `Option<String>` AND a `bool`** (highlighted in **blue**). The boolean is the crucial "was this the todo tool" flag we encountered directly in Chapter 3's nag logic. The option absolutely distinguishes "unknown tool" error from "known tool that produced empty output." **Never conflate these two genuinely different cases.**

2. **Unknown tools absolutely never panic** (highlighted in **green**). If the model invents a tool name — which it does, especially under stress situations — the harness gracefully returns the string `Unknown tool: {name}` and the loop continues smoothly. The model sees the clear error and typically picks the absolutely correct tool on the very next turn. A panic would immediately end the entire session with a cryptic stack trace that the model absolutely cannot read or recover from.

### The Deliberate Simplicity

**The dispatcher is deliberately, absolutely small.** Every tool gets a single `match` arm and a one-line function call. If you discover yourself writing complex routing logic inside the dispatcher, you are fundamentally conflating **routing** (which tool to call) with **running** (actually executing the tool) — split them clearly, and let the dispatcher perform only ONE thing.

---

## 4.7 The Parent/Child Tool Split: Role-Based Tool Sets

Examine carefully the definitions near the bottom of `tools.rs`:

```rust
pub fn child_agent_tools() -> Vec<Json> {
    vec![tool_bash(), tool_read_file(), tool_write_file(), tool_edit_file()]
}

pub fn parent_agent_tools() -> Vec<Json> {
    // Exactly the four child tools PLUS: tool_todo(), tool_subagent(), tool_background()
}
```

**The parent agent and its subagents see RADICALLY different tool sets.** Subagents receive only the **basic file tools**. ONLY the parent receives access to `todo`, `subagent` (for spawning further children), and the background-task tools.

### The Precise Reasoning

| Reason | Explanation |
|--------|-------------|
| **Subagents have inherently short lifetimes** | They are capped at approximately 30 turns, typically running much shorter. Planning tools are genuine overkill for these one-shot tasks. |
| **Only one level of subagents keeps the tree manageable** | If absolutely every subagent could spawn more subagents, you would get exponential fan-out explosions and completely runaway API billing costs. |
| **Fewer tools, faster responses** | The complete tool list gets inserted into absolutely EVERY prompt (see Chapter 3). Removing the three or four tools that a subagent genuinely does not need shaves dozens of precious tokens off every single LLM call. |

> **The Insight:** Tool sets are fundamentally a component of the agent's identity. A parent agent and a child subagent are **NOT two instances of the identical agent** — they are **two completely different agents with overlapping but distinct toolboxes**. Think about tool scoping deliberately — it is essentially a **free way** to give the model a considerably clearer idea of precisely **what role it is playing** at any given moment.

---

## 4.8 The Runners: Safety as an Absolute Non-Negotiable Priority

Each tool's runner lives in `tool_runners.rs`. They absolutely all begin with **the identical foundational pattern**:

```rust
pub fn run_read(path: &str, limit: Option<usize>, workdir: &WorkdirRoot)
    -> String
{
    let resolved = match safe_path(path, workdir) {
        Ok(p) => p,
        Err(e) => return format!("Error: {e}"),
    };
    // ... now actually read the file in safety ...
}
```

The `safe_path` function is the cornerstone of our **sandboxing strategy**: it takes a user-supplied path, joins it carefully to the designated workdir, canonicalizes the result to resolve any `..` or symlinks, and **absolutely refuses** any path that would escape the workdir root. We will spend the entirety of Chapter 5 comprehensively exploring how this sandboxing works and precisely why it is absolutely non-negotiable. The thing to absorb right now is the **essential shape**: **every single runner** starts by resolving paths through `safe_path`, and absolutely **every failure** returns a properly formatted string error rather than panicking or crashing.

### The Hardest Lesson in Agent Tool Design

**The single hardest lesson in all of agent tool design is this absolute principle:**

> **A tool runner MUST never panic, MUST never hang indefinitely, and MUST never modify state outside the workdir. Everything else is genuinely negotiable.**

This is NOT "should not." This is NOT "tries not to." It is an absolute **MUST NOT**:

- **Every panic** is an **immediately dead session** — the model cannot recover
- **Every hang** is an **immediately dead harness process** — needing manual restart
- **Every out-of-workdir write** is definitively a **security vulnerability** — potentially severe

The five runners in `rust_toy_agent` absolutely collectively enforce these three invariant rules, and **every production agent** you will ever examine enforces the identical three invariants.

---

## Chapter 4 Summary and Transition

In this chapter, we comprehensively covered:

1. **What precisely counts as a tool** — understanding the four essential components: name, description, input schema, and runner.

2. **The exactly-five-tool discipline** — appreciating precisely why fewer tools is genuinely better, and how `bash` can and should consume most of what other frameworks over-complicate.

3. **The edit_file design** — understanding why `old_text` / `new_text` literal replacement is the only format that actually works reliably at scale.

4. **Input schema shaping** — learning the critical rules: single-purpose tools, optional-without-defaults, and why pagination almost always belongs in `bash`.

5. **Output formatting** — understanding why explicit commands, exit codes, truncation notices, and line numbers in output absolutely matter.

6. **The dispatcher pattern** — appreciating how routing must remain trivially simple while the runners do all the real work.

7. **Parent/child tool splitting** — seeing how different agents can and should have deliberately different tool sets.

8. **The absolute safety invariants** — internalizing that panics, hangs, and out-of-workdir modifications are the three unforgivable sins.

In the **next chapter**, we will follow the paranoid thread begun here in Section 4.8 and comprehensively examine sandboxing: path traversal defense, canonicalization, environment variable allowlists, and the workdir root mechanism that anchors the entire system.

---

**Next:** Chapter 5 — Sandboxing, Path Safety, and the Workdir Root