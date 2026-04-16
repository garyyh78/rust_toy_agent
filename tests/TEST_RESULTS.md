# Test Results

## Test Run Log

| Round | Date | Time | Total Tests | Passed | Failed | Duration |
|-------|------|------|-------------|--------|--------|----------|
| 1 | 2026-04-16 | 09:00 | 212 | 212 | 0 | 2.18s |

### Round 1 Details (2026-04-16)

#### Unit Tests (src/lib.rs)
- **Total**: 205 tests
- **Result**: All passed
- **Duration**: 0.51s

#### Integration Tests (tests/cli_integration.rs)
- **Total**: 4 tests
- **Result**: All passed
- **Duration**: 0.62s

#### Integration Tests (tests/mock_llm_integration.rs)
- **Total**: 3 tests
- **Result**: All passed
- **Duration**: 1.05s

#### Doc Tests
- **Total**: 0 tests
- **Result**: N/A

---

## Previous Fixes Verified

Based on `todo.txt`, the following items are marked as [DONE]:

| Item | Description | Status |
|------|-------------|--------|
| [11] | src/worktree/manager.rs exceeds 350 LOC target | ✅ Verified (now 347 LOC) |
| [31] | Remove dead constant SWE_BENCH_INSTANCE_ID | ✅ Verified (not in src/) |
| [32] | Add pedantic clippy as non-blocking CI job | ✅ Verified (clippy_pedantic job exists in CI) |
| [33] | Fix doc_markdown warnings | ✅ Verified (cargo clippy -D warnings clean) |
| [34] | Add # Errors sections to Result-returning public fns | ✅ Verified |
| [35] | Add #[must_use] to pure getters and builder methods | ✅ Verified |
| [36] | Fix cast_possible_truncation warnings | ✅ Verified |
| [37] | Remaining uninlined_format_args | ✅ Verified |
| [38] | Apply map_or() where map().unwrap_or() is used | ✅ Verified |
| [39] | Flatten redundant_else blocks in main.rs | ✅ Verified |
| [40] | Fix single_char_pattern sites | ✅ Verified |
| [46] | Fix ptr_arg — &mut Vec<Json> → &mut [Json] | ✅ Verified |
| [47] | Harden scripts/install-hooks.sh | ✅ Verified (set -euo pipefail + show-toplevel) |
| [48] | Harden scripts/swe_bench/run_single_swe.sh | ✅ Verified (set -euo pipefail) |
| [49] | Extend pre-commit hook to run cargo fmt --check | ✅ Verified (cargo fmt --check in pre-commit) |
| [50] | Add shellcheck CI job | ✅ Verified (shellcheck job in CI) |
| [51] | Add code coverage job (cargo-llvm-cov) | ✅ Verified (llvm-cov job in CI) |

## Build & Clippy Status

| Command | Status |
|---------|--------|
| `cargo build` | ✅ Clean |
| `cargo test` | ✅ All 212 passing |
| `cargo clippy -- -D warnings` | ✅ Clean |
| `cargo clippy -- -W pedantic` | ⚠️ 158 warnings (acceptable per todo.txt baseline) |

## Remaining Work (Cancelled Items)

The following items were marked as [CANCELLED] in todo.txt:

- [41] bin_core/agent_loop.rs::agent_loop refactor
- [42] bin_core/dispatch.rs top-level function refactor
- [43] bin_core/teammate.rs top-level function refactor
- [44] src/agent_loop.rs directory module conversion
- [45] src/tools.rs directory module conversion
