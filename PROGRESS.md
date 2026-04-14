# Progress Summary

## Resolved Issues

### P0 (Ship Blockers)
- **[1]** Nested tokio Runtime panic in dispatch_tool "task" branch
  - Already fixed in earlier refactoring: `dispatch_tool` is `async`, "task" branch uses `state.subagent.run_subagent(prompt).await`
  - Verified: no `Runtime::new` in dispatch code

- **[5]** BackgroundManager runs each command TWICE
  - Fixed: replaced double `build_command().output()` calls with single call that captures both status and output
  - Removed `command_for_status` and `command_for_output` clones
  - Added `background_command_runs_exactly_once` regression test

- **[6]** TaskManager is not thread-safe across teammates
  - Fixed: `State.task_mgr` is now `Arc<Mutex<TaskManager>>`
  - Teammate threads share the same TaskManager via Arc clone instead of creating separate instances
  - Verified: single `TaskManager::new` call site in `State::new()`

- **[10]** Dead tokio runtime inside tokio thread (teammate_loop)
  - Fixed: `teammate_loop` is now `async fn`, uses `tokio::spawn` instead of `std::thread::spawn + Runtime::new`
  - All `.block_on()` calls replaced with direct `.await`
  - `thread::sleep` replaced with `tokio::time::sleep().await`
  - Verified: `grep Runtime::new src/` returns zero hits

### P1+ Items (Resolved in prior commits)
- **[2]** Subtract-overflow in validate_tool_pairing — early return for empty/single histories
- **[3]** UTF-8 byte-slice panics — `truncate_chars` in `text_util.rs`
- **[7]** Cargo aliases moved to `.cargo/config.toml`
- **[9]** save_test_result doc comment added; README corrected
- **[11-13]** AgentLoopHost trait, module refactor, bin_core extraction
- **[14,16,17]** Various structural fixes
- **[15]** Duplicate Task struct merged
- **[18]** Hardcoded "lead" string centralized to `LEAD` constant
- **[19,55]** Magic numbers centralized in `config.rs`
- **[20]** parking_lot::Mutex for non-poisonable locks
- **[21]** Error swallowing audit (let _ =, .ok())
- **[24,64]** truncate_messages pair-aware; validate_tool_pairing tests deduplicated
- **[25]** Tool error messages include file paths
- **[28-30]** Dependency hardening (chrono features, reqwest rustls-tls, tokio feature trim)
- **[31]** CI rust-cache + job splitting
- **[34-36]** .env guard, log rotation, rustfmt.toml + clippy.toml
- **[37]** .editorconfig
- **[38-41]** Test isolation (TempDir), CLI integration tests
- **[42-43]** Mock LLM server, proptest fuzzing
- **[44-46]** Security: symlink escape, env allowlist, bash timeout
- **[47-48]** Tracing migration, metrics collection
- **[49-52]** README fixes, CONTRIBUTING.md, CODE_OF_CONDUCT.md
- **[54]** Deduplicated `extract_final_text`
- **[60]** Fragile `Value` comparison in child_tools filter
- **[61]** Removed unused crossbeam-channel dependency
- **[62]** Cleaned up test-only warnings

### Remaining Low-Priority Items
- **[32]** CI release build + e2e smoke tests (nice-to-have, not blocking)
- **[42]** Mock LLM server tests (integration test infrastructure)
- **[63]** Orphan library-only modules (worktree, autonomous_agents)

## Build & Test Status
- `cargo build`: clean
- `cargo test`: 214 passed (210 unit + 4 integration)
- `cargo clippy -- -D warnings`: clean
- Zero `Runtime::new` calls remaining
- Zero double-execution patterns in background_tasks