# Rust Toy Agent

A minimal AI coding agent implementation in Rust that demonstrates the core agent loop pattern with a library-based architecture.

## Overview

An interactive coding assistant that uses the Anthropic API to run shell commands, read/write/edit files, and track multi-step tasks via a TodoManager. The codebase is organized into reusable library modules and a binary.

## The Agent Loop

The core pattern is simple:

```
while stop_reason == "tool_use":
    response = LLM(messages, tools)
    execute tools
    append results
```

This creates a feedback loop where the model can:
1. Request tool execution
2. See the results
3. Continue until it decides to stop

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                     Module Dependency Graph                   │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│   ┌─────────────┐        ┌──────────────┐                    │
│   │   client.rs │◄───────│ agent_loop   │                    │
│   └─────────────┘        └──┬───┬───┬───┘                    │
│        create_message()     │   │   │                        │
│                             │   │   │                        │
│   ┌─────────────┐           │   │   │                        │
│   │   tools.rs  │◄──────────┘   │   │                        │
│   └─────────────┘  dispatch      │   │                        │
│       dispatch_tools()           │   │                        │
│                                  │   │                        │
│   ┌─────────────┐               │   │                        │
│   │  logger.rs  │◄──────────────┘   │                        │
│   └─────────────┘  log_*()          │                        │
│                                     │                        │
│   ┌─────────────┐                   │                        │
│   │help_utils.rs│◄──────────────────┘                        │
│   └─────────────┘  (called by tools)                         │
│                                                              │
│   ┌──────────────┐                                           │
│   │ s03_todo_    │  Binary entry point                       │
│   │ write.rs     │  wires everything together                │
│   └──────────────┘                                           │
└──────────────────────────────────────────────────────────────┘
```

## Project Structure

```
src/
├── lib.rs              # Library root, exports all modules
├── client.rs           # AnthropicClient (API wrapper)
├── logger.rs           # Colored stderr logging helpers
├── help_utils.rs       # Path helpers & tool runners (bash, read, write, edit)
├── tools.rs            # TOOLS schema, TodoManager, dispatch_tools
└── s03_todo_write.rs   # Binary: main + REPL
```

### Library Modules

| Module | Purpose | Diagram |
|--------|---------|---------|
| `client` | `AnthropicClient` -- builds and sends requests to the Anthropic Messages API | Struct with `from_env()`, `new()`, `create_message()`, `build_request_body()` |
| `logger` | Colored stderr output: `log_section`, `log_info`, `log_step`, `log_output_preview` | Each function targets a different visual level |
| `help_utils` | Path sandboxing (`safe_path`, `normalize_path`) and tool runners | `safe_path` guards all runners; each returns String, never panics |
| `tools` | TOOLS JSON schema, `TodoManager`, `dispatch_tools` router | Routes tool names to help_utils runners; manages todo state |
| `agent_loop` | Core loop: call LLM, dispatch tools, track nag reminder | Ties client + tools + logger together |

## Features

- **5 Tools**: `bash`, `read_file`, `write_file`, `edit_file`, `todo`
- **TodoManager**: LLM-driven task tracking with status validation
- **Nag Reminder**: Injects `<reminder>` if the LLM skips todo updates for 3+ rounds
- **Safety**: Dangerous commands blocked, path escape prevention, 50KB output cap
- **Logging**: Color-coded stderr diagnostics via dedicated logger module
- **Interactive REPL**: Continuous conversation with colored output

## Usage

### Prerequisites

- Rust 1.70+
- Anthropic API key

### Setup

1. Copy the example environment file:
```bash
cp .env.example .env
```

2. Edit `.env` with your API key:
```
ANTHROPIC_API_KEY=your_key_here
MODEL_ID=claude-sonnet-4-20250514
```

3. Build and run:
```bash
cargo build --release
./target/release/s03_todo_write
```

### Interactive Session

```
╔══════════════════════════════════════════════════════════════╗
║          S03 Agent Loop - TodoWrite Edition                 ║
╚══════════════════════════════════════════════════════════════╝

  model        claude-sonnet-4-20250514
  workdir      /path/to/project
  tools        bash, read_file, write_file, edit_file, todo
  max_tokens   8000

s03 >> List files and create a hello world script
[Agent executes tools and shows results]
```

Type `q`, `exit`, or press Enter with empty input to quit.

## Testing

```bash
cargo test
```

Tests are split across modules:

| Module | Tests | Covers |
|--------|-------|--------|
| `help_utils` | 13 | Path normalization, safe_path, bash blocking, file read/write/edit |
| `client` | 7 | Request body construction, env defaults |
| `tools` | 14 | TOOLS schema, TodoManager validation, dispatch routing |
| `agent_loop` | 5 | Nag reminder, messages flow, stop reasons, system prompt, tool result structure |
| `logger` | 5 | Logging functions execute without panic |

## Linting

```bash
cargo fmt          # Auto-format
cargo clippy       # Lint check
cargo clippy --fix # Auto-fix lint issues
```

## License

MIT
