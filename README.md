# Rust Toy Agent

A minimal AI coding agent in Rust. It talks to the Anthropic API (or any compatible endpoint like DeepSeek), executes tools in a loop, and tracks multi-step tasks with a built-in todo system. Now includes advanced agent patterns: subagents, skill loading, context compression, agent teams, team protocols, autonomous agents, and git worktree isolation.

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
├── llm_client.rs          # AnthropicClient: API wrapper
├── logger.rs              # SessionLogger: stderr + file logging
├── tool_runners.rs        # Path sandboxing + tool runners
├── todo_manager.rs        # TodoManager: task tracking
├── tools.rs               # TOOLS schema + dispatch_tools router
├── agent_loop.rs          # Core loop, validation, truncation
├── subagent.rs            # Subagent system: spawn child agents with fresh context
├── skill_loading.rs       # Two-layer skill injection: metadata + on-demand loading
├── context_compact.rs     # Three-layer context compression pipeline
├── agent_teams.rs         # Agent teams with persistent named teammates + message bus
├── team_protocols.rs      # Shutdown and plan approval protocols with request tracking
├── worktree.rs            # Git worktree management with task isolation
├── background_tasks.rs    # Background task execution with notification queue
├── task_system.rs         # Persistent task management with dependency graph
└── e2e_test.rs            # End-to-end test runner with result tracking
```

### Module Responsibilities

| Module | What it does | Key exports |
|--------|-------------|-------------|
| `llm_client` | HTTP client for Anthropic Messages API | `AnthropicClient::from_env()`, `create_message()`, `send_body()` |
| `logger` | Dual-output logging (stderr with colors + plain text file) | `SessionLogger::new(path)`, `log_api_request()`, `log_api_response()` |
| `tool_runners` | Filesystem sandbox and tool runners | `safe_path()`, `run_bash()`, `run_read()`, `run_write()`, `run_edit()` |
| `todo_manager` | Task tracking with validation (max 20 items) | `TodoManager::new()`, `update()`, `render()`, `items()` |
| `tools` | Tool JSON schema and dispatch router | `TOOLS` const, `dispatch_tools()` |
| `agent_loop` | Core agent loop with validation, truncation, nag reminder | `agent_loop()`, `validate_tool_pairing()`, `truncate_messages()`, `call_llm()`, `dispatch_tool_calls()` |
| `subagent` | Spawn child agents with fresh context | `Subagent::new()`, `run_subagent()`, `agent_loop()` |
| `skill_loading` | Two-layer skill injection: metadata + on-demand loading | `SkillLoader::new()`, `get_descriptions()`, `get_content()` |
| `context_compact` | Three-layer context compression pipeline | `ContextCompactor::new()`, `micro_compact()`, `auto_compact()`, `compact()` |
| `agent_teams` | Persistent named teammates with thread-safe message bus | `MessageBus::new()`, `send()`, `read_inbox()`, `broadcast()`, `TeammateManager::new()`, `spawn()`, `list_all()` |
| `team_protocols` | Shutdown and plan approval with DashMap request tracking | `ProtocolTracker::new()`, `create_shutdown_request()`, `respond_shutdown()`, `submit_plan()`, `review_plan()` |
| `worktree` | Git worktree management with task binding and event bus | `WorktreeManager::new()`, `create()`, `remove()`, `keep()`, `run()`, `EventBus`, `TaskBinding` |
| `background_tasks` | Background task execution with notification queue | `BackgroundTaskRunner::new()`, `spawn()`, `submit()`, `poll()` |
| `task_system` | Persistent task management with dependency graph | `TaskSystem::new()`, `create_task()`, `update_status()`, `get_dependencies()` |
| `e2e_test` | End-to-end test runner with result tracking | `run_test()`, `load_test_case()`, `save_test_result()` |
| `bin_core` | Core binary components: REPL, state, dispatch | `run_repl()`, `AppState::new()`, `dispatch()` |
| `config` | Configuration loading from environment | `Config::from_env()`, `load()` |
| `metrics` | Round-level metrics collection and reporting | `RoundMetrics::new()`, `record()`, `summarize()` |
| `text_util` | Text utilities for token counting and truncation | `count_tokens()`, `truncate_text()`, `split_by_token_limit()` |

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
Available tests: `sum_1_to_n`, `fibonacci_sum`, `prime_sum`, `literary_style_detection`.

# Run all tests in task_tests/
for dir in task_tests/*/; do
    test_name=$(basename "$dir")
    cargo run -- --test "$test_name"
done
```

**Test Structure:**

```
task_tests/
├── sum_1_to_n/
│   └── test.json          # Test case definition
└── test_results/          # Auto-generated results
    └── sum_1_to_n_<timestamp>.json
```

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
| `tool_runners` | 16 | Path normalization, safe_path escapes, bash blocking, file CRUD |
| `llm_client` | 10 | Request body building, API error handling (401, 400, connection failure) |
| `todo_manager` | 17 | Validation, render format, update/replacement, boundary conditions |
| `tools` | 18 | TOOLS schema, dispatch routing |
| `agent_loop` | 36 | Nag reminder, tool pairing, message truncation, corrupted history, API error extraction |
| `logger` | 11 | SessionLogger file creation, timestamps, stderr output |
| `subagent` | 19 | Subagent creation, tool dispatch, child tools |
| `skill_loading` | 7 | Frontmatter parsing, skill loader, skill agent, system prompt |
| `context_compact` | 5 | Token estimation, micro_compact, tool dispatch, compactor creation |
| `agent_teams` | 16 | Message bus, send/read/broadcast, teammate manager, spawn, persistence |
| `team_protocols` | 17 | Shutdown protocol, plan approval, concurrent tracking, serialization |
| `worktree` | 20 | Event bus, index, task binding, name validation, worktree manager |
| `text_util` | 4 | Unicode-safe truncation with and without ellipsis |
| `bin_core::dispatch` | 5 | Tool dispatch routing, unknown/idle/compact handlers |

Total: **214 tests** (210 unit + 4 CLI integration). Run `cargo test` to verify.

## End-to-End Tests

The project includes E2E tests to evaluate the agent on real programming tasks. Each test runs the agent in a clean workspace, tracks execution time, token usage, and number of steps.

| Test | Language | Expected Output |
|------|----------|-----------------|
| `sum_1_to_n` | Python | 50005000 |
| `fibonacci_sum` | C++ | 2178308 |
| `prime_sum` | TypeScript | 3682913 |
| `literary_style_detection` | Python | "correct" |

Run tests:

```bash
# Run individual tests
cargo run --release -- --test sum_1_to_n
cargo run --release -- --test fibonacci_sum
cargo run --release -- --test prime_sum
cargo run --release -- --test literary_style_detection

# Or use aliases
cargo e2e-sum
cargo e2e-fib
cargo e2e-prime
```

### Latest Results (2026-04-02, deepseek-chat, commit `8c79bc7`)

| Test | Status | Time (ms) | Tokens |
|------|--------|-----------|--------|
| `sum_1_to_n` | PASS | 85,795 | 9,519 |
| `fibonacci_sum` | PASS | 130,229 | 19,231 |
| `prime_sum` | FAIL* | 128,188 | 23,708 |
| `literary_style_detection` | PASS | 189,549 | 42,004 |

**3/4 passed.** \*`prime_sum` produced the correct answer (3682913) but the agent's final text response was verbose instead of printing only the number.

Test results are saved to `task_tests/test_results/` with model name, commit hash, execution time, token counts, and step count.

## License

MIT
