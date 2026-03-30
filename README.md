# Rust Toy Agent

A minimal AI coding agent in Rust. It talks to the Anthropic API (or any compatible endpoint like DeepSeek), executes tools in a loop, and tracks multi-step tasks with a built-in todo system. Now includes advanced agent patterns: subagents, skill loading, and context compression.

## How It Works

```
User types a prompt
    │
    ▼
┌──────────────────────────────────────────────────────────┐
│  agent_loop (round N)                                    │
│    1. validate tool_use/tool_result pairing              │
│    2. truncate old messages (keep last 8 rounds)         │
│    3. build API request body                             │
│    4. log request JSON to session file                   │
│    5. POST to /v1/messages                               │
│    6. log response JSON to session file                  │
│    7. if stop_reason == "tool_use":                      │
│       a. dispatch each tool call                         │
│       b. collect tool_result blocks                      │
│       c. if 3+ rounds without todo: inject reminder      │
│       d. append results, goto 1                          │
│    8. else: return final text                            │
└──────────────────────────────────────────────────────────┘
```

## Project Structure

```
src/
├── main.rs             # Binary entry point (REPL)
├── lib.rs              # Library root (exports 9 modules)
├── client.rs           # AnthropicClient: API wrapper
├── logger.rs           # SessionLogger: stderr + file logging
├── help_utils.rs       # Path sandboxing + tool runners
├── todo_manager.rs     # TodoManager: task tracking
├── tools.rs            # TOOLS schema + dispatch_tools router
├── agent_loop.rs       # Core loop, validation, truncation
├── subagent.rs         # Subagent system: spawn child agents with fresh context
├── skill_loading.rs    # Two-layer skill injection: metadata + on-demand loading
└── context_compact.rs  # Three-layer context compression pipeline
```

### Module Responsibilities

| Module | What it does | Key exports |
|--------|-------------|-------------|
| `client` | HTTP client for Anthropic Messages API | `AnthropicClient::from_env()`, `create_message()`, `send_body()` |
| `logger` | Dual-output logging (stderr with colors + plain text file) | `SessionLogger::new(path)`, `log_api_request()`, `log_api_response()` |
| `help_utils` | Filesystem sandbox and tool runners | `safe_path()`, `run_bash()`, `run_read()`, `run_write()`, `run_edit()` |
| `todo_manager` | Task tracking with validation | `TodoManager::new()`, `update()`, `render()`, `items()` |
| `tools` | Tool JSON schema and dispatch router | `TOOLS` const, `dispatch_tools()` |
| `agent_loop` | Core agent loop with validation and truncation | `agent_loop()`, `validate_tool_pairing()`, `truncate_messages()` |
| `subagent` | Spawn child agents with fresh context | `Subagent::new()`, `run_subagent()`, `agent_loop()` |
| `skill_loading` | Two-layer skill injection system | `SkillLoader::new()`, `get_descriptions()`, `get_content()`, `SkillAgent` |
| `context_compact` | Three-layer context compression pipeline | `ContextCompactor::new()`, `micro_compact()`, `auto_compact()`, `agent_loop()` |

### Tools

| Tool | What it does |
|------|-------------|
| `bash` | Run shell commands (dangerous patterns blocked) |
| `read_file` | Read file contents, optional line limit |
| `write_file` | Write content to file, creates parent dirs |
| `edit_file` | Replace first occurrence of text in file |
| `todo` | Update task list (max 20 items, one in_progress at a time) |
| `task` | Spawn a subagent with fresh context (subagent module) |
| `load_skill` | Load specialized knowledge by name (skill_loading module) |
| `compact` | Trigger manual conversation compression (context_compact module) |

## Advanced Agent Patterns

### Subagent System (`subagent.rs`)

Spawn child agents with fresh context. The child works in its own context, sharing the filesystem, then returns only a summary to the parent.

```
Parent agent                     Subagent
+------------------+             +------------------+
| messages=[...]   |             | messages=[]      |  <-- fresh
|                  |  dispatch   |                  |
| tool: task       | ---------->| while tool_use:  |
|   prompt="..."   |            |   call tools     |
|   description="" |            |   append results |
|                  |  summary   |                  |
|   result = "..." | <--------- | return last text |
+------------------+             +------------------+
```

**Key insight**: "Process isolation gives context isolation for free."

### Skill Loading (`skill_loading.rs`)

Two-layer skill injection that avoids bloating the system prompt:

- **Layer 1 (cheap)**: Skill names in system prompt (~100 tokens/skill)
- **Layer 2 (on demand)**: Full skill body in tool_result

```
Skills directory structure:
skills/
  pdf/
    SKILL.md          <-- frontmatter (name, description) + body
  code-review/
    SKILL.md
```

**Key insight**: "Don't put everything in the system prompt. Load on demand."

### Context Compression (`context_compact.rs`)

Three-layer compression pipeline for long-running agents:

1. **Layer 1: micro_compact** (silent, every turn)
   - Replace tool_result content older than last 3 with "[Previous: used {tool_name}]"

2. **Layer 2: auto_compact** (when tokens > 50000)
   - Save full transcript to `.transcripts/`
   - Ask LLM to summarize conversation
   - Replace all messages with [summary]

3. **Layer 3: manual compact** (triggered by compact tool)
   - Same as auto, but triggered manually by the model

**Key insight**: "The agent can forget strategically and keep working forever."

## Safety Features

- **Path sandboxing**: `safe_path()` rejects paths that escape the workspace
- **Command blocking**: `run_bash()` blocks `rm -rf /`, `sudo`, `shutdown`, `reboot`, `> /dev/`
- **Output cap**: Tool output truncated to 50KB
- **History validation**: `validate_tool_pairing()` checks tool_use/tool_result matching before sending
- **Conversation truncation**: `truncate_messages()` keeps last 8 rounds to prevent API overflow
- **Error handling**: `create_message()` returns `Result` instead of panicking

## Session Logging

Every session writes to `logs/session_YYYYMMDD_HHMMSS.log` with:
- Full API request JSON (pretty-printed)
- Full API response JSON (pretty-printed)
- User input and agent responses
- Tool call details and outputs
- API errors with structured fields (message, type, code, param)
- Timestamps on every line

## Setup

```bash
cp .env.example .env
# Edit .env:
#   ANTHROPIC_API_KEY=your_key_here
#   ANTHROPIC_BASE_URL=https://api.anthropic.com   (or DeepSeek, etc.)
#   MODEL_ID=claude-sonnet-4-20250514

cargo build --release
./target/release/rust_toy_agent
```

Type `q`, `exit`, or press Enter to quit.

## Testing

```bash
cargo test           # Run all tests
cargo fmt            # Auto-format
cargo clippy         # Lint check
```

### End-to-End Tests

Run E2E tests to evaluate the agent on real programming tasks:

```bash
# Run a specific test
cargo run -- --test pi_series

# Run all tests in task_tests/
for dir in task_tests/*/; do
    test_name=$(basename "$dir")
    cargo run -- --test "$test_name"
done
```

**Test Structure:**

```
task_tests/
├── pi_series/
│   └── test.json          # Test case definition
└── test_results/          # Auto-generated results
    └── pi_series_<timestamp>.json
```

**Test JSON format:**

```json
{
  "name": "pi_series",
  "prompt": "Write a Python program to...",
  "expected_output": "expected result",
  "language": "python"
}
```

**Test Results include:**
- Model name and git commit hash
- Test timestamp
- Execution time (ms)
- Token count (input/output)

Results are auto-committed to git after each test run.

| Module | Tests | What's covered |
|--------|-------|----------------|
| `help_utils` | 13 | Path normalization, safe_path escapes, bash blocking, file CRUD |
| `client` | 10 | Request body building, API error handling (401, 400, connection failure) |
| `todo_manager` | 16 | Validation, render format, update/replacement, boundary conditions |
| `tools` | 8 | TOOLS schema, dispatch routing |
| `agent_loop` | 22 | Nag reminder, tool pairing, message truncation, corrupted history, API error extraction |
| `logger` | 5 | SessionLogger file creation, timestamps, stderr output |
| `subagent` | 3 | Subagent creation, tool dispatch, child tools |
| `skill_loading` | 7 | Frontmatter parsing, skill loader, skill agent, system prompt |
| `context_compact` | 5 | Token estimation, micro_compact, tool dispatch, compactor creation |

## License

MIT
