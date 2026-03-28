# Rust Toy Agent

A minimal AI coding agent in Rust. It talks to the Anthropic API (or any compatible endpoint like DeepSeek), executes tools in a loop, and tracks multi-step tasks with a built-in todo system.

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
├── lib.rs              # Library root (exports 5 modules)
├── client.rs           # AnthropicClient: API wrapper
├── logger.rs           # SessionLogger: stderr + file logging
├── help_utils.rs       # Path sandboxing + tool runners
├── tools.rs            # TOOLS schema, TodoManager, dispatch_tools
├── agent_loop.rs       # Core loop, validation, truncation
└── s03_todo_write.rs   # Binary: REPL entry point

logs/
└── session_YYYYMMDD_HHMMSS.log   # Auto-generated session transcript
```

### Module Responsibilities

| Module | What it does | Key exports |
|--------|-------------|-------------|
| `client` | HTTP client for Anthropic Messages API | `AnthropicClient::from_env()`, `create_message()`, `send_body()` |
| `logger` | Dual-output logging (stderr with colors + plain text file) | `SessionLogger::new(path)`, `log_api_request()`, `log_api_response()` |
| `help_utils` | Filesystem sandbox and tool runners | `safe_path()`, `run_bash()`, `run_read()`, `run_write()`, `run_edit()` |
| `tools` | Tool definitions, todo state management, dispatch router | `TOOLS` const, `TodoManager`, `dispatch_tools()` |
| `agent_loop` | Core agent loop with validation and truncation | `agent_loop()`, `validate_tool_pairing()`, `truncate_messages()` |

### Tools

| Tool | What it does |
|------|-------------|
| `bash` | Run shell commands (dangerous patterns blocked) |
| `read_file` | Read file contents, optional line limit |
| `write_file` | Write content to file, creates parent dirs |
| `edit_file` | Replace first occurrence of text in file |
| `todo` | Update task list (max 20 items, one in_progress at a time) |

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
- User input
- Agent responses
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
./target/release/s03_todo_write
```

Type `q`, `exit`, or press Enter to quit.

## Testing

```bash
cargo test           # Run all 65 tests
cargo fmt            # Auto-format
cargo clippy         # Lint check
```

| Module | Tests | What's covered |
|--------|-------|----------------|
| `help_utils` | 13 | Path normalization, safe_path escapes, bash blocking, file CRUD |
| `client` | 10 | Request body building, API error handling (401, 400, connection failure) |
| `tools` | 14 | TOOLS schema, TodoManager validation, dispatch routing |
| `agent_loop` | 22 | Nag reminder, tool pairing, message truncation, corrupted history detection, API error extraction |
| `logger` | 5 | SessionLogger file creation, timestamps, stderr output |

## License

MIT
