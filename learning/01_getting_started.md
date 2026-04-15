# Chapter 1: A Tour of a Coding Agent

> "The only way to learn a new programming language is by writing programs in it." — Brian Kernighan

The same principle applies to coding agents. You cannot learn how a
modern AI agent works from architecture diagrams alone; you have to read
code that runs, and then modify it. This book is an unhurried tour
through `rust_toy_agent`, a small but complete coding agent: it talks
to an LLM, dispatches the tool calls the model asks for, feeds results
back in a loop, and stops when the model says it is done. In ten
chapters we read the entire codebase, and by the end you will have a
working mental model of how every modern agent — Claude Code, Cursor,
Aider, Devin, and the one you will build next — is put together under
the hood.

This is not a Rust book. The code happens to be in Rust because Rust
keeps us honest about ownership, concurrency, and error handling, and
because a compiled static binary is a good model for a shippable agent.
You will pick up enough Rust to follow along, but every lesson is about
agent and harness engineering: what state an agent needs, how it talks
to tools, how to survive flaky networks, how to keep it from escaping
its sandbox, and how to ship the whole thing without it melting under
real workloads.

## 1.1 What We Are Building Toward

`rust_toy_agent` is a program that wraps a language-model API in a
tool-calling loop. On each round it sends the conversation history to
the model, reads back the `tool_use` blocks, runs them against the
local filesystem or shell, and appends the results as `tool_result`
blocks for the next round. The agent can read and edit files, run
bash commands, spawn subagents with fresh context, launch background
jobs, and maintain an explicit todo list that survives across rounds.

Each chapter tackles one piece of agent engineering:

* **Chapter 2 — Working memory.** Why an agent needs an explicit
  scratchpad, and how `TodoManager` turns a simple list into a plan
  the model can follow.
* **Chapter 3 — The agent loop.** The tool-use / tool-result
  protocol, termination conditions, and the invariants that keep
  the conversation history valid.
* **Chapter 4 — Tool design.** The contract between the LLM and the
  local machine: what tools to expose, how to shape their inputs
  and outputs, and how they fail.
* **Chapter 5 — Sandboxing.** Canonical paths, workdir roots, bash
  environment allowlists — the unglamorous safety rails that keep
  a confused agent from deleting `~/.ssh`.
* **Chapter 6 — Context management.** Token budgets, history
  truncation, message pairing, and the subtle rules that keep the
  conversation coherent over hundreds of turns.
* **Chapter 7 — Robust LLM I/O.** Transient versus fatal errors,
  exponential backoff with jitter, timeouts, and why you must
  never trust the network.
* **Chapter 8 — Prompt engineering in code.** System prompts, tool
  descriptions, and the "nag reminder" pattern that pulls a
  drifting agent back on task.
* **Chapter 9 — Subagents and background work.** Spawning child
  contexts, fire-and-forget commands, and when to decompose a
  task versus drive it to completion in one mind.
* **Chapter 10 — Observability and shipping.** Session logs,
  metrics, git worktree isolation, and what changes when the agent
  stops being a demo and starts being a tool other people use.

You do not need to know Rust. If you can read Python or Go or
TypeScript, you will keep up. The interesting content is in the
shape of the code, not the syntax — and when a Rust idiom gets in
the way, the text translates it.

## 1.2 Setting Up Your Environment

Rust is distributed through a one-line installer called `rustup`:

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

That gives you three commands worth remembering:

| command | purpose |
| --- | --- |
| `rustc` | the compiler itself; you will rarely call it directly |
| `cargo` | the package manager and build tool; this is what you *will* call |
| `rustup` | manages toolchain versions and components (clippy, rustfmt) |

Every Rust project is a **Cargo package**, described by a `Cargo.toml` file
at the root. Here is the one for `rust_toy_agent`, trimmed to its essentials:

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

A few details worth noting:

* **`edition = "2021"`** selects the language edition. Editions are how
  Rust makes non-backwards-compatible language changes without breaking
  old code: old crates keep using their declared edition, new ones opt
  into newer features.
* **`[lib]`** tells Cargo this package exposes a library crate in
  addition to (or instead of) a binary. The library's entry point is
  `src/lib.rs`.
* **Feature flags** like `default-features = false` strip heavyweight
  options from a dependency. `reqwest` without default features pulls
  in roughly half as much code.

## 1.3 Building and Running the Project

Clone the repo, then from its root:

```
cargo build              # compile
cargo build --release    # compile with optimizations, slower but faster code
cargo run -- --help      # run the binary, passing --help as an argument
cargo test               # run every #[test] in the project
cargo clippy             # run the linter
cargo fmt                # format the code
```

The `--` in `cargo run -- --help` separates arguments to `cargo` from
arguments to the program it runs. You will see this pattern everywhere.

When you run `cargo build` for the first time, Cargo downloads each
dependency listed in `Cargo.toml`, compiles it, and caches the result
under `target/`. Subsequent builds are incremental.

## 1.4 The Project Layout

Here is the directory structure of `rust_toy_agent`, simplified:

```
src/
├── main.rs                  # binary entry point (thin wrapper)
├── lib.rs                   # library root (declares all modules)
├── llm_client.rs            # HTTP client for the API
├── tool_runners.rs          # filesystem and shell tool implementations
├── agent_loop.rs            # the main loop that talks to the model
├── todo_manager.rs          # task-tracking state (our Chapter-2 example)
├── subagent.rs              # spawn child agents with fresh context
├── background_tasks.rs      # fire-and-forget shell commands
├── worktree/                # git worktree management (a directory module!)
│   ├── mod.rs
│   ├── binding.rs
│   ├── events.rs
│   ├── index.rs
│   └── manager.rs
└── bin_core/                # pieces of the binary REPL
```

Two of those entries deserve immediate explanation:

1. **`main.rs` vs `lib.rs`**. A Cargo package with both files produces
   *two* crates: a library (`lib.rs`) and a binary (`main.rs`) that
   depends on the library. This is an extremely common pattern: the
   library holds all the real logic, which makes it testable, and the
   binary is a thin shell that parses arguments and calls library
   functions. Any code in `main.rs` is inaccessible to integration
   tests; code in `lib.rs` is.

2. **Directory modules** like `worktree/`. Rust lets you expand a
   module into a directory by replacing `worktree.rs` with a folder
   containing `mod.rs`. Everything in the folder becomes a submodule.
   We cover this in depth in Chapter 4, but note now that `src/worktree/`
   started life as a single 900-line file and was broken up for
   readability.

## 1.5 A Fifteen-Second Look at Real Rust

Here is `TodoItem`, the smallest real struct in the project. Don't worry
about understanding every token yet — just absorb the shape of the language.

```rust
// from src/todo_manager.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoItem {
    pub id: String,
    pub text: String,
    pub status: String,
}
```

Observations:

* `pub` makes the struct and its fields visible outside the current
  module. Private by default is a Rust value.
* `#[derive(...)]` is an **attribute**. It tells the compiler to
  auto-generate implementations of the listed traits (`Debug` for
  pretty-printing, `Clone` for `.clone()`, `PartialEq`/`Eq` for `==`).
  Attributes are ubiquitous; we'll meet many more.
* `String` is Rust's growable, heap-allocated, UTF-8 string. Its
  borrowed counterpart is `&str`. You will spend some time in Chapter 2
  understanding when to pick which.
* There is no constructor syntax built into the language. Instead, the
  convention is a function called `new` in an `impl` block. Here is
  the matching one:

```rust
impl TodoManager {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }
}
```

`Self` is an alias for the surrounding type's name — if you rename the
struct, the `impl` block still compiles.

## 1.6 Using Cargo Effectively

A few habits that will save you time:

* **Run `cargo check` instead of `cargo build`** when you only care
  about whether the code type-checks. `check` skips the code-generation
  step and is several times faster.
* **Keep `cargo clippy -- -D warnings` in your feedback loop.** Clippy
  is an extra linter that catches a large number of stylistic and
  correctness issues. The `-D warnings` flag upgrades its warnings to
  errors, which is what this project uses in CI.
* **Read `cargo test` output carefully.** Rust's test output shows each
  test name, and failing tests print the assertion message along with
  the source line. You will learn the codebase faster by running its
  tests than by reading its code linearly.

Try this now, from the repo root:

```
cargo test todo_manager
```

You should see about seventeen tests pass. Their names are your table
of contents for the next chapter.

## 1.7 Exercises

1. Run `cargo build` from the repo root. How long does a clean build
   take on your machine? Run it again. How long does the incremental
   build take?
2. Open `Cargo.toml` and find a dependency you don't recognize. Search
   for it on `https://crates.io/` and read its one-line summary. Add
   your notes in a scratch file.
3. Run `cargo test llm_client` and count how many tests pass. These
   are the tests we will dissect in Chapter 6.
4. Run `cargo tree -e normal --depth 1`. This prints the direct
   dependency graph. How many crates does `rust_toy_agent` depend on
   directly?
5. Peek at `src/main.rs`. It should be short — under 100 lines. Notice
   how it defers almost everything to `bin_core`. Speculate about
   *why* the maintainers chose this layout; we will confirm your guess
   in Chapter 4.

In the next chapter we meet `TodoManager` in full and use it to ask
a question that every agent designer faces on day one: what should
the model *remember*, and where should that memory live?
