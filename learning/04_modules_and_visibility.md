# Chapter 4: Tool Design — The Contract With the Model

> "The best programs are written so that computing machines can perform them quickly and so that human beings can understand them clearly." — *Donald Knuth*

## Introduction

Tools are the agent's hands. What the harness exposes defines its capabilities, failure modes, and personality. This chapter covers selecting and shaping tools.

## 4.1 What Precisely Counts as a Tool

A tool has exactly four components:

| Component | Description | Example |
|-----------|-------------|---------|
| Name | Short string the model sees | `"bash"`, `"read_file"` |
| Description | One or two sentences | `"Run a shell command."` |
| Input Schema | JSON Schema for arguments | `{"type": "object", ...}` |
| Runner | Actual harness code | The `run_bash` function |

The JSON tool definition for `bash`:

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

The description is four words. The model already knows what a shell command is. A lengthy paragraph would dilute the model's attention.

**Calibration rule:** Description length should scale with how **surprising** or **unusual** the tool is, NOT with how important it is.

## 4.2 The Exactly Five Tools

`rust_toy_agent` exposes five tools:

| Tool Name | Input | Output | Purpose |
|-----------|-------|--------|---------|
| `bash` | `command` (string) | stdout + stderr | Universal escape hatch |
| `read_file` | `path`, `limit?` | File contents | Inspect code and text |
| `write_file` | `path`, `content` | Confirmation | Create files |
| `edit_file` | `path`, `old_text`, `new_text` | Confirmation | Modify files |
| `todo` | `items[]` | Rendered list | Externalized planning |

**Five tools, not fifteen.** Every tool added expands the system prompt, adds failure modes, and creates attention competition.

**Why Bash eats everything:** `find`, `grep`, `ls`, `git status` — all are one-liners accessible through `bash`. Adding a dedicated `grep` tool is usually a regression.

**Why read_file exists separately:** The harness intelligently truncates line counts. Critically, `read_file` is idempotent and safe — NO shell interpretation, NO wildcard expansion.

**Why write_file and edit_file exist:** Writing through `bash` requires heredocs or escaped printf. The model gets these wrong often enough that dedicated tools pay for themselves.

## 4.3 The edit_file Decision

Input schema:

```json
"required": ["path", "old_text", "new_text"]
```

Replaces the first occurrence of `old_text` with `new_text`. No regex, no line number dependencies, no complex patch format.

### Why This Format Wins

- **Line numbers are terrible.** Models don't count well. Line numbers shift between turns.

- **Regex patches invite catastrophic bugs.** Unescaped special characters make the edit silently become a no-op or match too much.

- **Unified diffs consume too many tokens.** A six-byte change becomes a twenty-line diff blob.

- **Direct AST edits are unscalable.** Works for one codebase, breaks for the next.

The `old_text` / `new_text` format sidesteps all these issues. The model quotes enough context to make `old_text` unique, the harness performs literal `str::replacen(old_text, new_text, 1)`.

The critical gotcha: if `old_text` is not unique, only the first occurrence is replaced. Failing loudly is almost always superior to doing something that seems plausible but may be incorrect.

## 4.4 Shaping Inputs

```json
"properties": {
  "path": {"type": "string"},
  "limit": {"type": "integer"}
},
"required": ["path"]
```

Three observations:

1. **`path` is a single string, NOT an array.** Keep tools single-purpose and let the model make N independent calls. The protocol supports multiple tool_use blocks per round.

2. **`limit` is optional with NO default value.** The model decides precisely when it wants a bounded read. Defaulting makes two wrong choices at once.

3. **No `offset` field.** No pagination support. The model can use `bash` with `sed -n` for its own pagination.

**North Star Principle:** Every argument should have a clear, non-overlapping job. If you have six or seven optional fields, you probably have two or three separate tools fused together.

## 4.5 Shaping Outputs

Tool output is a string — the `content` field of a `tool_result`.

`run_bash` returns:

```
$ git status
On branch main
Your branch is up to date with 'origin/main'.

nothing to commit, working tree clean

(exit 0)
```

Three essential features:

- **The command is echoed at the top** — the model knows which call produced which output
- **Stdout and stderr are clearly interleaved** — many tools write status to stderr
- **Exit code is explicit** — `(exit 0)` or `(exit 1)`. Never rely on the model guessing.

`read_file` outputs:

```
src/agent_loop.rs (lines 1-200 of 400 total, 12,345 bytes):
   1  //! agent_loop.rs
   2  ...
```

Includes line numbers on every line, the specific range, and byte count.

**Truncation notice:** `MAX_TOOL_OUTPUT_BYTES` is 50 KB. When truncated, output ends with:

```
... (output truncated, 1,234,567 bytes total)
```

The model needs to know truncation occurred and how much data was dropped.

## 4.6 The Dispatcher

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

Two observations:

1. **Returns BOTH `Option<String>` AND a `bool`.** The boolean is the "was this the todo tool" flag for nag logic.

2. **Unknown tools never panic.** If the model invents a tool name, the harness returns `Unknown tool: {name}`.

The dispatcher is deliberately small. Every tool gets one match arm and a one-line function call.

## 4.7 Parent/Child Tool Split

```rust
pub fn child_agent_tools() -> Vec<Json> {
    vec![tool_bash(), tool_read_file(), tool_write_file(), tool_edit_file()]
}

pub fn parent_agent_tools() -> Vec<Json> {
    // Four child tools PLUS: tool_todo(), tool_subagent(), tool_background()
}
```

Subagents receive only basic file tools. Only the parent gets `todo`, `subagent`, and background-task tools.

| Reason | Explanation |
|--------|-------------|
| Short lifetimes | ~30 turns maximum. Planning tools are overkill. |
| Prevent fan-out | If every subagent could spawn more, exponential billing explosion |
| Fewer tokens | Removing tools shaves precious tokens off every LLM call |

Tool sets are a component of the agent's identity. A parent and child are two completely different agents with distinct toolboxes.

## 4.8 The Runners: Safety as Priority

Every runner begins with the identical pattern:

```rust
pub fn run_read(path: &str, limit: Option<usize>, workdir: &WorkdirRoot) -> String {
    let resolved = match safe_path(path, workdir) {
        Ok(p) => p,
        Err(e) => return format!("Error: {e}"),
    };
    // ... now actually read the file ...
}
```

`safe_path` is the cornerstone of sandboxing: joins to workdir, canonicalizes, refuses paths that escape. Chapter 5 explores this comprehensively.

**The hardest lesson:** A tool runner MUST never panic, MUST never hang indefinitely, and MUST never modify state outside the workdir.

- Every panic is an immediately dead session
- Every hang is an immediately dead harness process
- Every out-of-workdir write is a security vulnerability

---

**Next:** Chapter 5 — Sandboxing, Path Safety, and the Workdir Root