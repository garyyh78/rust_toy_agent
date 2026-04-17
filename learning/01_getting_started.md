# Chapter 1: A Tour of a Coding Agent

> "The only way to learn a new programming language is by writing programs in it." — *Brian Kernighan*

## Introduction

The same principle applies to coding agents: you cannot learn how a modern AI agent works by studying architecture diagrams; you must read code that executes, run it, and modify it to see how it responds.

`rust_toy_agent` performs these critical operations: communicates with an LLM, dispatches tool calls the model requests, feeds results back into the conversation, and stops when the model signals completion. By the end of this book, you will have a working mental model of how every modern agent system is constructed — whether it's Claude Code, Cursor, Aider, Devin, or whatever you build next.

This is **not** a book about Rust. The code happens to be in Rust because it enforces strict rules about ownership, concurrency, and error handling. Every lesson is about agent engineering: what state an agent needs, how it communicates with tools, how it survives flaky network conditions, how to prevent it from escaping its sandbox, and how to ship the complete system.

## 1.1 The Agent Architecture

`rust_toy_agent` wraps a language-model API in a structured tool-calling loop:

1. **Send** the conversation history to the language model
2. **Receive** `tool_use` blocks specifying which tools to invoke
3. **Execute** those tool calls against the filesystem or shell
4. **Append** `tool_result` blocks back to the conversation history
5. **Repeat** until the model signals completion

The agent can read and edit files, execute bash commands, spawn subagents, launch background jobs, and maintain a todo list.

## 1.2 Setting Up Your Development Environment

### Installing Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

This configures your entire Rust toolchain. You will use three commands:

| Command | Purpose |
|---------|---------|
| `rustc` | The Rust compiler |
| `cargo` | Package manager and build tool |
| `rustup` | Manages toolchain versions |

### Understanding Cargo.toml

Every meaningful Rust project is a Cargo package, described by `Cargo.toml`:

```toml
[package]
name = "rust_toy_agent"
version = "0.1.0"
edition = "2021"

[lib]
name = "rust_toy_agent"
path = "src/lib.rs"

[dependencies]
reqwest  = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
tokio    = { version = "1",    features = ["rt-multi-thread", "macros", "time"] }
serde_json = "1"
anyhow   = "1"
git2     = "0.19"
```

`edition = "2021"` selects the language edition. `[lib]` tells Cargo this package exposes a library with entry point at `src/lib.rs`. Feature flags like `default-features = false` strip heavyweight options from dependencies.

## 1.3 Building and Running the Project

```bash
git clone <repository-url>
cd rust_toy_agent
```

Essential commands:

| Command | Meaning |
|---------|---------|
| `cargo build` | Compile from source |
| `cargo build --release` | Compile with full optimizations |
| `cargo run -- --help` | Execute binary with arguments |
| `cargo test` | Run all `#[test]` functions |
| `cargo clippy` | Execute extra linter |
| `cargo fmt` | Format code to conventions |

The `--` separator in `cargo run -- --help` separates cargo arguments from program arguments.

When you run `cargo build` for the first time, Cargo downloads dependencies from crates.io, compiles them, and caches everything under `target/`. Subsequent builds are faster due to incremental compilation.

## 1.4 The Complete Project Layout

```
src/
├── main.rs                  # Binary entry point
├── lib.rs                   # Library root
├── llm_client.rs            # HTTP client for LLM API
├── tool_runners.rs          # Filesystem and shell tools
├── agent_loop.rs          # Central loop
├── todo_manager.rs         # Task-tracking state
├── subagent.rs           # Spawn child agents
├── background_tasks.rs    # Fire-and-forget shell handling
├── worktree/             # Git worktree management
│   ├── mod.rs
│   ├── binding.rs
│   ├── events.rs
│   ├── index.rs
│   └── manager.rs
└── bin_core/             # Binary-specific pieces
```

### main.rs vs lib.rs

A package with both `main.rs` and `lib.rs` produces two crates: a library defined by `lib.rs` and a binary defined by `main.rs` that depends on the library. The library holds all substantive logic; the binary only parses arguments and delegates.

### Directory Modules

Rust allows expanding a module into a directory by replacing a file like `worktree.rs` with a folder named `worktree/` containing at minimum an `mod.rs` file.

## 1.5 A Fifteen-Second Introduction to Real Rust

Here is `TodoItem`, the smallest meaningful struct:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoItem {
    pub id: String,
    pub text: String,
    pub status: String,
}
```

- `pub` makes the struct and fields visible outside the module. Rust enforces strict privacy by default.
- `#[derive(...)]` is an attribute that auto-generates implementations for traits: `Debug` enables pretty-printing, `Clone` provides `.clone()`, `PartialEq` and `Eq` enable equality comparison.
- `String` is Rust's growable, heap-allocated, UTF-8 string type. Its borrowed counterpart is `&str`.

There is no built-in constructor syntax. The convention is a function named `new` inside an `impl` block:

```rust
impl TodoManager {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }
}
```

`Self` is a shorthand for the enclosing type's name. `Vec::new()` creates a new empty vector.

## 1.6 Using Cargo Effectively

**Run `cargo check`** instead of `cargo build` when you only want to verify type checking. It skips code generation and runs faster.

**Incorporate `cargo clippy -- -D warnings`** into your regular feedback loop. The `-D warnings` flag promotes all warnings to errors, ensuring clean code.

**Read `cargo test` output carefully.** Rust's test framework displays each test name individually, and failing tests print the assertion message and exact source line.

Try this command from the project root:

```bash
cargo test todo_manager
```

You should see approximately seventeen tests pass. Their names serve as a table of contents for Chapter 2.

---

**Next:** Chapter 2 — Structs, Ownership, and Working Memory