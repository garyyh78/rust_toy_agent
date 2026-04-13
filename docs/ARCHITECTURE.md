# Rust Toy Agent - Component Diagram

```text
┌─────────────────────────────────────────────────────────────────────────────────┐
│                                    main.rs                                       │
│   Entry point: wires modules, runs REPL loop, or --test e2e mode                │
│                                                                                  │
│   ┌─────────────┐   ┌──────────────┐   ┌───────────────┐   ┌────────────────┐  │
│   │ agent_loop  │   │ llm_client   │   │     tools      │   │ todo_manager   │  │
│   └─────┬───────┘   └──────────────┘   └───────┬────────┘   └────────────────┘  │
│         │                                       │                                 │
│         │                                   tool_runners                          │
└─────────┼───────────────────────────────────────┼─────────────────────────────────┘
          │                                       │
          ▼                                       ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              CORE AGENT LAYER                                    │
│                                                                                  │
│  ┌──────────────────────────────────────────────────────────────────────────┐    │
│  │                          agent_loop.rs                                    │    │
│  │  Main loop: validate → truncate → call LLM → dispatch tools → iterate   │    │
│  │                                                                          │    │
│  │  Flow:                                                                   │    │
│  │    loop {                                                                │    │
│  │      1. validate_tool_pairing()  — ensure tool_use/tool_result pairs     │    │
│  │      2. truncate_messages()      — keep last 8 rounds                    │    │
│  │      3. call_llm()               — send to Anthropic API                 │    │
│  │      4. parse response           — extract stop_reason + content         │    │
│  │      5. dispatch_tool_calls()    — route to tools.rs                     │    │
│  │      6. maybe_inject_nag()       — remind LLM to update todos (3+ skip)  │    │
│  │      7. append tool_results      — loop until stop_reason != tool_use    │    │
│  │    }                                                                     │    │
│  └──────────────────┬───────────────────┬───────────────────┬───────────────┘    │
│                     │                   │                   │                     │
│                     ▼                   ▼                   ▼                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐     │
│  │  llm_client  │  │    tools     │  │   logger     │  │  todo_manager    │     │
│  │  .rs         │  │    .rs       │  │   .rs        │  │  .rs             │     │
│  └──────────────┘  └──────┬───────┘  └──────────────┘  └──────────────────┘     │
└────────────────────────────┼─────────────────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              LLM CLIENT                                         │
│                                                                                  │
│  ┌──────────────────────────────────────────────────────────────────────────┐    │
│  │                          llm_client.rs                                    │    │
│  │  AnthropicClient                                                         │    │
│  │  ├── from_env()              — reads ANTHROPIC_API_KEY, ANTHROPIC_BASE_URL│    │
│  │  ├── new(api_key, base_url)  — explicit credentials                      │    │
│  │  ├── build_request_body()    — pure JSON builder (model, system, msgs,   │    │
│  │  │                            tools, max_tokens)                         │    │
│  │  ├── create_message()        — async POST /v1/messages                   │    │
│  │  └── send_body()             — send pre-built request body               │    │
│  └──────────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                          TOOL DISPATCH LAYER                                    │
│                                                                                  │
│  ┌─────────────────────────────┐    ┌─────────────────────────────────────┐     │
│  │         tools.rs            │    │           tool_runners.rs           │     │
│  │                             │    │                                     │     │
│  │  TOOLS constant (JSON)      │    │  Path helpers:                      │     │
│  │  ├── bash                   │───▶│  ├── normalize_path() → safe_path() │     │
│  │  ├── read_file              │    │                                     │     │
│  │  ├── write_file             │    │  Tool runners:                      │     │
│  │  ├── edit_file              │    │  ├── run_bash()   — sh -c, 50KB cap │     │
│  │  └── todo                   │    │  ├── run_read()   — read N lines    │     │
│  │                             │    │  ├── run_write()  — write + mkdir   │     │
│  │  dispatch_tools()           │    │  └── run_edit()   — replacen(old,1) │     │
│  │  Routes by name → handler   │    └─────────────────────────────────────┘     │
│  │  Returns (output, did_todo) │                                               │
│  └──────────────┬──────────────┘                                               │
│                 │                                                               │
│                 ▼                                                               │
│  ┌─────────────────────────────┐                                               │
│  │       todo_manager.rs       │                                               │
│  │                             │                                               │
│  │  TodoManager                │                                               │
│  │  ├── items: Vec<TodoItem>   │                                               │
│  │  ├── update() → validate    │  max 20 items, one in_progress,               │
│  │  │   └── max 20, one        │  non-empty text, valid status enum            │
│  │  │       in_progress        │                                               │
│  │  └── render() → display     │  [ ] pending, [>] in_progress, [x] completed  │
│  └─────────────────────────────┘                                               │
└─────────────────────────────────────────────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                              LOGGING                                            │
│                                                                                  │
│  ┌──────────────────────────────────────────────────────────────────────────┐    │
│  │                          logger.rs                                        │    │
│  │                                                                          │    │
│  │  Free functions (stderr only):                                           │    │
│  │  ├── log_section()  ├── log_info()  ├── log_step()                      │    │
│  │  └── log_output_preview()                                                │    │
│  │                                                                          │    │
│  │  SessionLogger (stderr + file):                                          │    │
│  │  ├── new(path)           — create log file, append mode                  │    │
│  │  ├── stderr_only()       — no file logging                               │    │
│  │  ├── log_session_start() — model + workdir                               │    │
│  │  ├── log_user_input()    — file only (already shown in prompt)           │    │
│  │  ├── log_agent_response()— file only                                     │    │
│  │  ├── log_api_request()   — full JSON to file                             │    │
│  │  ├── log_api_response()  — full JSON to file                             │    │
│  │  └── log_api_error()     — error string to file                          │    │
│  └──────────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                          MULTI-AGENT LAYER                                      │
│                                                                                  │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────────────────┐   │
│  │  autonomous_     │  │  agent_teams.rs  │  │  subagent.rs                │   │
│  │  agents.rs       │  │                  │  │                              │   │
│  │                  │  │  TeammateManager │  │  Subagent                    │   │
│  │  Task lifecycle: │  │  ├── config.json │  │  ├── child_tools (4 tools)  │   │
│  │  spawn → WORK    │  │  ├── spawn()     │  │  ├── parent_tools (+ task)  │   │
│  │       → IDLE     │  │  ├── list_all()  │  │  ├── run_subagent()         │   │
│  │       → shutdown │  │  └── set_status()│  │  │   fresh context, 30 limit │   │
│  │                  │  │                  │  │  └── agent_loop()            │   │
│  │  StatusManager   │  │  MessageBus      │  │      delegates via "task"   │   │
│  │  └── HashMap     │  │  ├── send()      │  │      tool                   │   │
│  │      statuses    │  │  ├── read_inbox()│  └──────────────────────────────┘   │
│  │                  │  │  └── broadcast() │                                     │
│  │  scan_unclaimed_ │  │                  │  ┌──────────────────────────────┐   │
│  │  tasks()         │  │  Message         │  │  skill_loading.rs            │   │
│  │  claim_task()    │  │  ├── msg_type    │  │                              │   │
│  │  watch_tasks_    │  │  ├── from        │  │  SkillLoader                 │   │
│  │  dir() (notify)  │  │  ├── content     │  │  ├── Layer 1: descriptions   │   │
│  │  poll_for_work() │  │  └── timestamp   │  │  │   (cheap, in system prompt)│   │
│  └──────────────────┘  └──────────────────┘  │  ├── Layer 2: full body      │   │
│                                              │  │   (on demand, tool_result) │   │
│  ┌───────────────────────────────────────┐   │  ├── SkillAgent              │   │
│  │  background_tasks.rs                  │   │  │   └── load_skill tool     │   │
│  │                                       │   │  └── parse_frontmatter()     │   │
│  │  BackgroundManager                    │   └──────────────────────────────┘   │
│  │  ├── run(command)  — fire & forget    │                                     │
│  │  │   └── thread::spawn               │                                     │
│  │  ├── check(task_id) — status poll    │                                     │
│  │  └── drain_notifications()            │                                     │
│  │       └── Vec<Notification>           │                                     │
│  └───────────────────────────────────────┘                                     │
└─────────────────────────────────────────────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                        CONTEXT & PROTOCOLS                                      │
│                                                                                  │
│  ┌───────────────────────────────────────┐  ┌───────────────────────────────┐   │
│  │  context_compact.rs                   │  │  team_protocols.rs            │   │
│  │                                       │  │                               │   │
│  │  ContextCompactor                     │  │  ProtocolTracker              │   │
│  │  ├── Layer 1: micro_compact()         │  │  ├── DashMap (lock-free)      │   │
│  │  │   Replace old tool results with    │  │  │                             │   │
│  │  │   "[Previous: used <tool>]"        │  │  ├── Shutdown protocol:       │   │
│  │  │                                   │  │  │   create → respond → check  │   │
│  │  ├── Layer 2: auto_compact()          │  │  │   pending → approved/reject │   │
│  │  │   Save transcript → LLM summary   │  │  │                             │   │
│  │  │   → replace messages              │  │  ├── Plan approval protocol:  │   │
│  │  │                                   │  │  │   submit → review → check   │   │
│  │  └── Layer 3: manual_compact()        │  │  │   pending → approved/reject │   │
│  │      Triggered by "compact" tool      │  │  │                             │   │
│  │                                       │  │  └── request_id correlation   │   │
│  │  estimate_tokens() — ~4 chars/token   │  │      between request/response │   │
│  └───────────────────────────────────────┘  └───────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                        TASK & WORKTREE LAYER                                    │
│                                                                                  │
│  ┌───────────────────────────────────────┐  ┌───────────────────────────────┐   │
│  │  task_system.rs                       │  │  worktree.rs                  │   │
│  │                                       │  │                               │   │
│  │  TaskManager                          │  │  WorktreeManager              │   │
│  │  ├── .tasks/task_{id}.json            │  │  ├── create()  — git worktree │   │
│  │  ├── create(subject, description)     │  │  │   add -b wt/{name}        │   │
│  │  ├── get(task_id)                     │  │  ├── remove()  — git worktree │   │
│  │  ├── update(task_id, status, deps)    │  │  │   remove --force           │   │
│  │  │   ├── status transitions           │  │  ├── keep()    — mark without │   │
│  │  │   ├── blocked_by / blocks          │  │  │   removing                 │   │
│  │  │   └── clear_dependency on done     │  │  ├── status()  — git status   │   │
│  │  └── list_all()                       │  │  ├── run()     — exec in dir  │   │
│  │                                       │  │  └── list_all()              │   │
│  │  Task (serde JSON)                    │  │                               │   │
│  │  ├── id, subject, description         │  │  WorktreeIndex                │   │
│  │  ├── status (pending/in_progress/     │  │  ├── .worktrees/index.json    │   │
│  │  │            completed)              │  │  ├── find(), add(),           │   │
│  │  ├── blocked_by: Vec<u32>             │  │  │   update_status()          │   │
│  │  ├── blocks: Vec<u32>                 │  │  └── list_all()              │   │
│  │  └── owner: String                    │  │                               │   │
│  │                                       │  │  TaskBinding                  │   │
│  └───────────────────────────────────────┘  │  ├── bind(task_id, wt_name)  │   │
│                                              │  ├── unbind(task_id)          │   │
│                                              │  └── complete(task_id)        │   │
│                                              │                               │   │
│                                              │  EventBus                     │   │
│                                              │  ├── .worktrees/events.jsonl  │   │
│                                              │  ├── emit(event, task, wt)    │   │
│                                              │  └── list_recent(limit)       │   │
│                                              │                               │   │
│                                              │  Uses: git2 crate             │   │
│                                              └───────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                          E2E TESTING                                            │
│                                                                                  │
│  ┌──────────────────────────────────────────────────────────────────────────┐    │
│  │                          e2e_test.rs                                      │    │
│  │                                                                          │    │
│  │  TestCase        TestResult                                              │    │
│  │  ├── name        ├── name, model, commit, test_time                     │    │
│  │  ├── prompt      ├── passed, steps                                      │    │
│  │  ├── expected    ├── actual_output, expected_output                      │    │
│  │  └── language    └── total_time_ms, total_tokens, in/out_tokens         │    │
│  │                                                                          │    │
│  │  load_test_case(path)   — parse test.json                               │    │
│  │  run_test()             — execute agent_loop, collect metrics            │    │
│  │  print_test_result()    — formatted console output                       │    │
│  │  save_test_result()     — JSON to test_results/ dir                      │    │
│  └──────────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Module Dependency Graph

```text
main.rs
  ├── agent_loop
  │     ├── llm_client
  │     ├── tools
  │     │     ├── tool_runners
  │     │     └── todo_manager
  │     ├── logger
  │     └── todo_manager
  ├── llm_client
  ├── logger
  ├── tools
  └── e2e_test
        ├── agent_loop
        ├── llm_client
        ├── logger
        ├── todo_manager
        └── tools

autonomous_agents  ← uses notify crate, serde
agent_teams        ← uses serde, std::sync::RwLock
subagent           ← uses llm_client, tool_runners
skill_loading      ← uses llm_client, tool_runners
context_compact    ← uses llm_client, tool_runners
background_tasks   ← uses serde, std::thread
task_system        ← uses serde, std::fs
team_protocols     ← uses dashmap, uuid
worktree           ← uses git2 crate
```

## Key External Dependencies

| Crate | Used By | Purpose |
|-------|---------|---------|
| `reqwest` | `llm_client` | HTTP client for Anthropic API |
| `serde` / `serde_json` | All modules | JSON serialization |
| `notify` | `autonomous_agents` | Filesystem watching for task board |
| `dashmap` | `team_protocols` | Lock-free concurrent request tracking |
| `uuid` | `team_protocols`, `background_tasks` | Request/task ID generation |
| `git2` | `worktree` | Programmatic git operations |
| `tokio` | `main`, `agent_loop` | Async runtime |
| `chrono` | `main`, `e2e_test` | Timestamp formatting |
| `dotenvy` | `main` | Load .env files |
