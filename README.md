# Rust Toy Agent

A minimal AI coding agent implementation in Rust that demonstrates the core agent loop pattern.

## Overview

This is a Rust port of a Python-based AI agent that uses the DeepSeek API to create an interactive coding assistant. The agent can execute shell commands and iteratively improve its responses based on tool outputs.

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
- DeepSeek API key

### Setup

1. Copy the example environment file:
```bash
cp .env.example .env
```

2. Edit `.env` with your API key:
```
DEEPSEEK_API_KEY=your_key_here
MODEL_ID=deepseek-chat
```

3. Build and run:
```bash
cargo build --release
./target/release/s01_agent_loop
```

### Interactive Session

```
╔══════════════════════════════════════════════════════════════╗
║          S01 Agent Loop - Interactive Session                ║
╚══════════════════════════════════════════════════════════════╝

  model        deepseek-chat
  workdir      /path/to/project
  tools        bash
  max_tokens   8000

s01 >> ls -la
[Agent executes commands and shows results]
```

Type `q`, `exit`, or press Enter with empty input to quit.

## Code Structure

- `src/lib.rs` - Shared utilities (API client, bash execution, file tools)
- `src/s01_agent_loop.rs` - Agent loop implementation with tests

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

### Test Results

```
running 11 tests
test tests::test_exit_commands ... ok
test tests::test_messages_append_structure ... ok
test tests::test_run_bash_dangerous_blocked ... ok
test tests::test_stop_reason_check ... ok
test tests::test_system_prompt_format ... ok
test tests::test_tool_result_structure ... ok
test tests::test_tool_use_block_parsing ... ok
test tests::test_tools_json_parsing ... ok
test tests::test_run_bash_simple_echo ... ok
test tests::test_run_bash_no_output ... ok
test tests::test_run_bash_captures_stderr ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

## License

MIT