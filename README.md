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

## Demo

```
$ cargo run --release
> Write a Python program to sum 1 to n

[Agent uses bash, write_file tools...]
Done: created sum.py

> Run it

[Agent runs the program...]
Result: 5050
```

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

## Safety

- Path sandboxing prevents escaping the workspace
- Dangerous commands blocked: `rm -rf /`, `sudo`, etc.
- Output truncated at 50KB
- **Run in a sandboxed or disposable environment**

## Requirements

- Rust 1.75+
- Anthropic API key (or compatible endpoint)

## Installation

### From source

```bash
cargo build --release
./target/release/rust_toy_agent
```

### From GitHub releases

Download pre-built binaries from the releases page.

## Documentation

- [Configuration](docs/configuration.md)
- [Architecture](docs/architecture.md)
- [Testing](docs/testing.md)
- [Security Model](docs/security-model.md)

## Development

```bash
cargo test           # Run tests
cargo fmt --check    # Check formatting
cargo clippy         # Lint check
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

## License

MIT