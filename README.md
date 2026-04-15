# Rust Toy Agent

A minimal AI coding agent in Rust. It talks to the Anthropic API (or any compatible endpoint like DeepSeek), executes tools in a loop, and tracks multi-step tasks with a built-in todo system. Includes subagents, skill loading, context compression, agent teams, team protocols, and git worktree isolation.

## How It Works

```
User types a prompt
    │
    ▼
┌──────────────────────────────────────────────────────────┐
│  agent_loop (round N)                                    │
│    1. validate tool_use/tool_result pairing              │
│    2. truncate old messages (keep last 8 rounds)         │
│    3. build API request body                             │
│    4. log request JSON to session file                   │
│    5. POST to /v1/messages                               │
│    6. log response JSON to session file                  │
│    7. if stop_reason == "tool_use":                      │
│       a. dispatch each tool call                         │
│       b. collect tool_result blocks                      │
│       c. if 3+ rounds without todo: inject reminder      │
│       d. append results, goto 1                          │
│    8. else: return final text                            │
└──────────────────────────────────────────────────────────┘
```

## Project Structure

```
src/
├── main.rs                # Binary entry point (REPL)
├── lib.rs                 # Library root (exports 19 modules)
├── llm_client.rs          # AnthropicClient: API wrapper with retry + jitter
├── logger.rs              # SessionLogger: stderr + file logging
├── tool_runners.rs         # Path sandboxing (WorkdirRoot) + tool runners + dispatch_basic_file_tool
├── todo_manager.rs        # TodoManager: task tracking (max 20 items)
├── tools.rs                # TOOLS schema + dispatch_tools router
├── agent_loop.rs           # Core loop, validation, truncation, nag reminder
├── subagent.rs             # Subagent system: spawn child agents with fresh context
├── skill_loading.rs         # Skill metadata + on-demand loading (SkillLoader)
├── context_compact.rs      # Three-layer context compression pipeline
├── agent_teams.rs          # Agent teams with persistent named teammates + message bus
├── team_protocols.rs        # Shutdown and plan approval protocols with request tracking
├── background_tasks.rs     # Background task execution with DashMap + mpsc notifications
├── task_system.rs          # Persistent task management with dependency graph
├── e2e_test.rs              # End-to-end test runner with result tracking
├── worktree/                # Git worktree management (directory module)
│   ├── mod.rs               # Re-exports
│   ├── binding.rs           # TaskBinding
│   ├── events.rs            # EventBus, WorktreeEvent
│   ├── index.rs             # WorktreeIndex
│   ├── git.rs               # Git2 helper functions
│   └── manager.rs           # WorktreeManager (git2 calls)
└── bin_core/
    ├── mod.rs               # Module root
    ├── agent_loop.rs         # Full agent loop with tool dispatch
    ├── constants.rs          # Agent name constants
    ├── dispatch.rs           # Full agent tool dispatch router
    ├── repl.rs               # Interactive REPL
    ├── state.rs              # Full agent state struct
    ├── teammate.rs           # Teammate loop implementation
    └── test_mode.rs          # Test mode runner
```

### Directory Usage

| Directory/File | Purpose | Key Contents |
|----------------|---------|--------------|
| `src/` | Main source code | All Rust modules |
| `src/bin_core/` | Binary core components | REPL, state, dispatch, test mode |
| `src/worktree/` | Git worktree management | WorktreeManager, TaskBinding, EventBus |
| `tests/` | Integration tests | CLI and mock LLM tests |
| `task_tests/` | E2E test cases | JSON test definitions + results |
| `scripts/` | Utility scripts | Hooks, SWE-bench runners |
| `scripts/git-hooks/` | Git hooks | Pre-commit hook |
| `scripts/swe_bench_data/` | SWE-bench data | Repository clones for benchmarks |
| `scripts/swe_bench_results/` | SWE-bench outputs | Prediction files |
| `logs/` | Session logs | Auto-generated log files |
| `learning/` | Learning/data | Agent learning artifacts |

### Module Responsibilities

| Module | What it does | Key exports |
|--------|-------------|-------------|
| `llm_client` | HTTP client for Anthropic Messages API with retry and jitter | `AnthropicClient::from_env()`, `create_message()`, `send_body()`, `with_max_retries()` |
| `logger` | Dual-output logging (stderr with colors + plain text file) | `SessionLogger::new(path)`, `log_api_request()`, `log_api_response()` |
| `tool_runners` | Filesystem sandbox (WorkdirRoot), tool runners, and shared file dispatch | `WorkdirRoot::new()`, `safe_path()`, `dispatch_basic_file_tool()`, `run_bash()`, `run_read()`, `run_write()`, `run_edit()` |
| `todo_manager` | Task tracking with validation (max 20 items) | `TodoManager::new()`, `update()`, `render()`, `items()` |
| `tools` | Tool JSON schema and dispatch router | `TOOLS` const, `dispatch_tools()` |
| `agent_loop` | Core agent loop with validation, truncation, nag reminder | `agent_loop()`, `validate_tool_pairing()`, `truncate_messages()`, `call_llm()`, `dispatch_tool_calls()` |
| `subagent` | Spawn child agents with fresh context | `Subagent::new()`, `run_subagent()` |
| `skill_loading` | Skill metadata + on-demand loading | `SkillLoader::new()`, `get_descriptions()`, `get_content()` |
| `context_compact` | Three-layer context compression pipeline | `ContextCompactor::new()`, `micro_compact()`, `auto_compact()`, `compact()` |
| `agent_teams` | Persistent named teammates with thread-safe message bus | `MessageBus::new()`, `send()`, `read_inbox()`, `broadcast()`, `TeammateManager::new()`, `spawn()`, `list_all()` |
| `team_protocols` | Shutdown and plan approval with DashMap request tracking | `ProtocolTracker::new()`, `create_shutdown_request()`, `respond_shutdown()`, `submit_plan()`, `review_plan()` |
| `worktree` | Git worktree management with task binding and event bus | `WorktreeManager::new()`, `create()`, `remove()`, `keep()`, `run()`, `EventBus`, `TaskBinding` |
| `background_tasks` | Background task execution with DashMap + mpsc notification queue | `BackgroundManager::new()`, `run()`, `check()`, `drain_notifications()` |
| `task_system` | Persistent task management with dependency graph | `TaskSystem::new()`, `create_task()`, `update_status()`, `get_dependencies()` |
| `e2e_test` | End-to-end test runner with result tracking | `run_test()`, `load_test_case()`, `save_test_result()` |
| `bin_core` | Core binary components: REPL, state, dispatch | `run_repl()`, `AppState::new()`, `dispatch()` |
| `config` | Centralized constants and tuning knobs | `MAX_TOOL_OUTPUT_BYTES`, `LEAD_MAX_TOKENS`, `BASH_ENV_ALLOWLIST`, etc. |
| `metrics` | Round-level metrics collection and reporting | `RoundMetrics::new()`, `record()`, `summarize()` |
| `text_util` | Unicode-safe string truncation utilities | `truncate_chars()`, `truncate_chars_ellipsis()` |

### Tools

| Tool | What it does |
|------|-------------|
| `bash` | Run shell commands (dangerous patterns blocked) |
| `read_file` | Read file contents, optional line limit |
| `write_file` | Write content to file, creates parent dirs |
| `edit_file` | Replace first occurrence of text in file |
| `todo` | Update task list (max 20 items, one in_progress at a time) |
| `task` | Spawn a subagent with fresh context (subagent module) |
| `load_skill` | Load specialized knowledge by name (skill_loading module) |
| `compact` | Trigger manual conversation compression (context_compact module) |

## Safety Features

- **Path sandboxing**: `safe_path()` rejects paths that escape the workspace
- **Command blocking**: `run_bash()` blocks `rm -rf /`, `sudo`, `shutdown`, `reboot`, `> /dev/`
- **Output cap**: Tool output truncated to 50KB
- **History validation**: `validate_tool_pairing()` checks tool_use/tool_result matching before sending
- **Conversation truncation**: `truncate_messages()` keeps last 8 rounds to prevent API overflow
- **Error handling**: `create_message()` returns `Result` instead of panicking

## Session Logging

Every session writes to `logs/session_YYYYMMDD_HHMMSS.log` with:
- Full API request JSON (pretty-printed)
- Full API response JSON (pretty-printed)
- User input and agent responses
- Tool call details and outputs
- API errors with structured fields (message, type, code, param)
- Timestamps on every line

## Setup

```bash
cp .env.example .env
# Edit .env:
#   ANTHROPIC_API_KEY=your_key_here
#   ANTHROPIC_BASE_URL=https://api.anthropic.com   (or DeepSeek, etc.)
#   MODEL_ID=claude-sonnet-4-20250514

cargo build --release
./target/release/rust_toy_agent
```

Type `q`, `exit`, or press Enter to quit.

## Testing

```bash
cargo test           # Run all tests
cargo fmt            # Auto-format
cargo clippy         # Lint check
```

### End-to-End Tests

Run E2E tests to evaluate the agent on real programming tasks:

```bash
# Run a specific test
cargo run -- --test sum_1_to_n
Available tests: sum_1_to_n, fibonacci_sum, prime_sum, literary_style_detection, and more in task_tests/

# Run all tests using the script
./scripts/e2e/run_all.sh

# Run manually
for dir in task_tests/*/; do
    test_name=$(basename "$dir")
    if [ -f "$dir/test.json" ]; then
        cargo run -- --test "$test_name"
    fi
done
```

**Test Structure:**

```
task_tests/
├── sum_1_to_n/
│   ├── test.json          # Test case definition
│   └── sum_integers.py    # Test input files
├── bug_fix/
│   ├── test.json
│   └── buggy_sort.py
├── test_results/          # Auto-generated results
│   └── sum_1_to_n_<timestamp>.json
└── ...
```

**Available Tests:**
- `api_mock` - JSON parsing
- `bug_fix` - Fix palindrome sorting bugs
- `csv_transform` - CSV data processing
- `dependency_resolve` - Topological sort
- `fibonacci_sum` - C++ Fibonacci sum
- `graph_bfs` - Rust BFS implementation
- `literary_style_detection` - Author style detection
- `multiline_transform` - Text transformation
- `prime_sum` - TypeScript prime calculation
- `regex_extractor` - Email extraction
- `sum_1_to_n` - Python sum program

**Test JSON format:**

```json
{
  "name": "sum_1_to_n",
  "prompt": "Write a Python program to...",
  "expected_output": "expected result",
  "language": "python"
}
```

**Test Results include:**
- Model name and git commit hash
- Test timestamp
- Execution time (ms)
- Token count (input/output)

- Results are written to `task_tests/test_results/` (gitignored). Curate and commit worthwhile runs manually.

| Module | Tests | What's covered |
|--------|-------|----------------|
| `tool_runners` | 16 | Path normalization, safe_path escapes, WorkdirRoot, bash blocking, file CRUD |
| `llm_client` | 7 | Request body building, API error handling (401, 400, connection failure) |
| `todo_manager` | 17 | Validation, render format, update/replacement, boundary conditions |
| `tools` | 18 | TOOLS schema, dispatch routing |
| `agent_loop` | 36 | Nag reminder, tool pairing, message truncation, corrupted history, API error extraction |
| `logger` | 11 | SessionLogger file creation, timestamps, stderr output |
| `subagent` | 17 | Subagent creation, tool dispatch, child tools |
| `skill_loading` | 4 | Frontmatter parsing, skill loader |
| `context_compact` | 5 | Token estimation, micro_compact, tool dispatch, compactor creation |
| `agent_teams` | 16 | Message bus, send/read/broadcast, teammate manager, spawn, persistence |
| `team_protocols` | 17 | Shutdown protocol, plan approval, concurrent tracking, serialization |
| `worktree` | 20 | Event bus, index, task binding, name validation, worktree manager |
| `text_util` | 4 | Unicode-safe truncation with and without ellipsis |
| `background_tasks` | 3 | Task creation, notification drain, exactly-once execution |
| `task_system` | 3 | Task creation, dependency tracking, status management |
| `bin_core::state` | 2 | State creation, tools count |
| `bin_core::dispatch` | 5 | Tool dispatch routing, unknown/idle/compact/worktree handlers |
| `bin_core::constants` | 1 | LEAD constant |

Total: **205 unit tests + 4 CLI integration + 3 mock-LLM integration = 212**. Run `cargo test` to verify.

## License

MIT
