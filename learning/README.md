# Learning Rust Through Building a Coding Agent

A concise tour of `rust_toy_agent`, a small but complete coding agent implementation written in Rust.

## Chapters

1. **A Tour of a Coding Agent** — Architecture overview, setting up Rust
2. **Working Memory** — TodoManager, externalized planning
3. **The Agent Loop** — Tool-use/tool-result protocol, termination
4. **Tool Design** — Tool contracts, input/output shaping
5. **Sandboxing** — Path safety, workdir root, environment allowlists
6. **Context Management** — Truncation, compaction, prompt caching
7. **Robust LLM I/O** — Error classification, exponential backoff, jitter
8. **Prompt Engineering** — System prompts, tool descriptions, nag reminders
9. **Subagents and Background Work** — Spawning children, fire-and-forget tasks
10. **Observability and Shipping** — Session logging, metrics, worktree isolation

## Prerequisites

- Rust installed (`rustup`)
- Basic programming knowledge

## Use

```bash
cargo test todo_manager
```

Read the chapters in order. Each builds on the previous.