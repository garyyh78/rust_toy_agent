# Contributing

## Setup
- Rust 1.75+ (MSRV)
- Copy `.env.example` → `.env` and add your `ANTHROPIC_API_KEY` and `MODEL_ID`.
- Run `scripts/install-hooks.sh` to install the pre-commit hook that blocks secret leaks.

## Running tests
- `cargo test` — unit + integration tests
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`

## Running E2E tests
- `cargo run --release -- --test sum_1_to_n` — live E2E (requires API key)

## Commit style
- One logical change per commit.
- First line: imperative, <70 chars.
- Reference todo.txt item numbers when applicable, e.g. `[12] refactor agent_loop into trait`.

## Pull requests
- Keep PRs focused. Splitting large refactors into ordered commits is preferred over a single mega-PR.
- CI must be green (fmt + clippy + tests)