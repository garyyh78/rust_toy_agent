# Rust Toy Agent

A minimal AI coding agent implementation in Rust that demonstrates the core agent loop pattern.

## Overview

This is a Rust port of a Python-based AI agent that uses the Anthropic API to create an interactive coding assistant. The agent can execute shell commands and iteratively improve its responses based on tool outputs.

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
User Prompt → LLM → Tool Execute → Results
     ↑                               |
     └───────────────────────────────┘
```

## Features

- **Single Tool**: `bash` - Execute shell commands
- **Safety**: Dangerous commands are blocked (rm -rf /, sudo, shutdown, reboot, etc.)
- **Timeouts**: 120-second command timeout
- **Output Limits**: 50KB max output per command
- **Interactive REPL**: Continuous conversation with the agent

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
./target/release/rust_toy_agent
```

### Interactive Session

```
╔══════════════════════════════════════════════════════════════╗
║          S01 Agent Loop - Interactive Session                ║
╚══════════════════════════════════════════════════════════════╝

  model        claude-sonnet-4-20250514
  workdir      /path/to/project
  tools        bash
  max_tokens   8000

s01 >> ls -la
[Agent executes commands and shows results]
```

Type `q`, `exit`, or press Enter with empty input to quit.

## Code Structure

- `src/lib.rs` - Shared utilities (API client, bash execution, file tools)
- `src/main.rs` - Agent loop implementation with tests
- `s01_agent_loop.py` - Original Python version for reference

## Testing

Run the test suite:

```bash
cargo test
```

Tests cover:
- Tool JSON structure parsing
- Bash command execution
- Dangerous command blocking
- Output handling
- Message history structure

## Comparison with Python

| Feature | Python | Rust |
|---------|--------|------|
| Tool Support | bash only | bash only |
| Safety Checks | ✓ | ✓ |
| Timeout | 120s | 120s |
| Output Limit | 50KB | 50KB |
| Async | No | Yes (tokio) |

## License

MIT