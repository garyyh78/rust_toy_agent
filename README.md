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
User Prompt → LLM → Tool Dispatch → Results
     ↑                               |
     └───────────────────────────────┘
              |
     TodoManager tracks progress
     Nag reminder after 3 idle rounds
```

## Project Structure

```
src/
├── lib.rs              # Library root, exports all modules
├── client.rs           # AnthropicClient (API wrapper)
├── help_utils.rs       # Path helpers & tool runners (bash, read, write, edit)
├── tools.rs            # TOOLS schema, TodoManager, dispatch_tools
└── s03_todo_write.rs   # Binary: agent loop with nag reminder
```

### Library Modules

| Module | Purpose |
|--------|---------|
| `client` | `AnthropicClient` -- builds and sends requests to the Anthropic Messages API |
| `help_utils` | Path sandboxing (`safe_path`, `normalize_path`) and tool runners (`run_bash`, `run_read`, `run_write`, `run_edit`) |
| `tools` | TOOLS JSON schema, `TodoManager`, `dispatch_tools` router |

## Features

- **5 Tools**: `bash`, `read_file`, `write_file`, `edit_file`, `todo`
- **TodoManager**: LLM-driven task tracking with status validation
- **Nag Reminder**: Injects `<reminder>` if the LLM skips todo updates for 3+ rounds
- **Safety**: Dangerous commands blocked, path escape prevention, 50KB output cap
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
| `s03_todo_write` | 4 | Nag reminder threshold, message flow, tool result structure |

## License

MIT
