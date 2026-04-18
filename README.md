# Rust Toy Agent

An AI coding agent in Rust that executes tools in a loop to complete multi-step tasks.

## Quickstart

```bash
# 1. Clone and setup
git clone https://github.com/yugary/rust_toy_agent
cd rust_toy_agent
cp .env.example .env

# 2. Add your API key (edit .env)
ANTHROPIC_API_KEY=sk_...

# 3. Run
cargo run --release
```

Type `q` or press Enter to quit.

## Features

- **Interactive REPL**: Chat with the agent in a terminal
- **Tool-use**: Agent can run commands, read/write files, manage task lists
- **Test mode**: Run the agent on benchmark tasks (`--test <name>`)
- **Subagents**: Spawn child agents with fresh context
- **Skill loading**: Load domain-specific knowledge on demand
- **Context compression**: Handle long conversations
- **Team support**: Multiple named teammates with message passing
- **Worktree isolation**: Work in isolated git worktrees

## Tools

| Tool | Description |
|------|-------------|
| `bash` | Run shell commands |
| `read_file` | Read file contents |
| `write_file` | Create/update files |
| `edit_file` | Modify files |
| `todo` | Task tracking (max 20 items) |
| `task` | Spawn a subagent |
| `load_skill` | Load skill by name |
| `compact` | Compress conversation |

## Architecture

```
src/
├── main.rs              # Binary entry (REPL)
├── lib.rs               # Library root
├── llm_client.rs        # API client (Anthropic)
├── logger.rs            # Session logging
├── agent_loop.rs        # Core agent loop
├── tools.rs             # Tool schema + dispatch
├── tool_runners.rs      # Tool execution
├── context_compact.rs   # Context compression
├── subagent.rs          # Subagent spawning
├── background_tasks.rs  # Fire-and-forget tasks
└── worktree/            # Git worktree management
```

### Execution Flow

```
User prompt
    │
    ▼
agent_loop (round N)
    1. validate tool pairings
    2. truncate old messages
    3. build API request
    4. POST to API
    5. if tool_use:
       dispatch tools
       collect results
       continue loop
    ▼
Final response
```

## Configuration

### Environment Variables

Create a `.env` file from the template:

```bash
cp .env.example .env
```

| Variable | Description | Default |
|----------|-------------|---------|
| `ANTHROPIC_API_KEY` | API key for Anthropic (or compatible) | - |
| `MODEL_ID` | Model to use | `claude-sonnet-4-20250514` |
| `ANTHROPIC_BASE_URL` | API endpoint | `https://api.anthropic.com` |
| `ANTHROPIC_API_VERSION` | API version | `2023-06-01` |

### Requirements

- Rust 1.75+ (MSRV)
- Anthropic API key (or compatible endpoint)

### OS Support

- macOS (Intel, Apple Silicon)
- Linux (Ubuntu 22.04+)
- FreeBSD

## Security Model

This project executes arbitrary shell commands. **Run in a sandboxed or disposable environment.**

### Threat Model

**In Scope:**
- Shell command injection via LLM-generated commands
- API key exposure
- File system access beyond workspace
- Long-running processes

**Out of Scope:**
- Social engineering
- Physical access
- Third-party services beyond API provider

### Mitigations

**Path Sandboxing:** `safe_path()` rejects paths that escape the workspace:
- `/etc/passwd`
- `../../etc/passwd`
- `$HOME/.ssh/id_rsa`

**Command Blocklist:** These patterns are blocked:
- `rm -rf /`, `rm -rf ~`
- `sudo`, `su`
- `shutdown`, `reboot`
- `> /dev/null` (output suppression)

**Output Limits:** Tool output truncated at 50KB.

**Conversation Truncation:** Last 8 rounds kept to prevent API overflow.

### Recommendations

1. Run in a sandbox: Disposable VM, container, or isolated directory
2. Separate API keys: Use dedicated keys for agent work
3. Review commands: Before running agent-output commands
4. Limit permissions: Agent should only access what's needed
5. Monitor logs: Check `logs/` for activity

## Testing

```bash
cargo test           # All tests (~205 unit tests)
cargo test <name>    # Specific test

# E2E benchmark tests
cargo run --release -- --test sum_1_to_n
cargo run --release -- --test bug_fix

# Development
cargo fmt --check    # Check formatting
cargo clippy         # Lint check
```

Tests are in `tests/` and `task_tests/`.

## Documentation

See `learning/` directory for the book on building coding agents.

## License

MIT