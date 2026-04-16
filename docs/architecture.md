# Architecture

## Core Components

```
src/
├── main.rs              # Binary entry (REPL)
├── lib.rs              # Library root
├── llm_client.rs       # API client (Anthropic)
├── logger.rs           # Session logging
├── agent_loop.rs       # Core agent loop
├── tools.rs           # Tool schema + dispatch
├── tool_runners.rs     # Tool execution
└── bin_core/         # Binary components
    ├── repl.rs        # Interactive mode
    ├── test_mode.rs   # Test mode
    └── state.rs     # App state
```

## Execution Flow

```
User prompt
    │
    ▼
agent_loop (round N)
    1. validate tool pairings
    2. truncate old messages
    3. build API request
    4. POST to API
    5. if tool_use:
       dispatch tools
       collect results
       continue loop
    ▼
Final response
```

## Tool System

Tools are defined in `tools.rs` with JSON schema. Execution goes through `tool_runners.rs`.

## Persistence

- Task state: `task_system/`
- Worktrees: `worktree/`
- Logs: `logs/session_*.log`

## Safety

- Path sandboxing: `tool_runners::safe_path()`
- Command blocklist in `config.rs`