# Testing

## Running Tests

```bash
cargo test           # All tests
cargo test <name>    # Specific test
```

## Test Structure

- **Unit tests**: In-source with `#[cfg(test)]` modules
- **Integration tests**: `tests/` directory

## Test Categories

### Unit Tests

Run with normal `cargo test`:

```bash
cargo test           # ~205 unit tests
```

### Integration Tests

Located in `tests/`:
- `cli_integration.rs`: CLI behavior
- `mock_llm_integration.rs`: Mocked API tests

## E2E Tests

Run benchmark tasks:

```bash
cargo run --release -- --test sum_1_to_n
cargo run --release -- --test bug_fix
```

Available tests in `task_tests/`.

## Development Commands

```bash
cargo fmt            # Format
cargo fmt --check    # Check formatting
cargo clippy        # Lint
cargo clippy -- -D warnings  # Strict lint
```

## Coverage

```bash
cargo install cargo-llvm-cov
cargo llvm-cov --all-targets
```

## CI

CI runs on PRs and includes:
- fmt check
- clippy (1.75, stable)
- tests (1.75, stable)
- audit
- gitleaks