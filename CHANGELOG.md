# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-04-16

### Added
- Initial release with:
  - AnthropicMessages API client with retry + jitter
  - Tool system: bash, read_file, write_file, edit_file, todo, task, load_skill, compact
  - Interactive REPL mode
  - Test mode (`--test`)
  - SWE-bench benchmark support
  - Subagent spawning
  - Skill loading system
  - Context compression (3-layer pipeline)
  - Agent teams with teammate system
  - Team protocols (shutdown, plan approval)
  - Background task execution
  - Persistent task system
  - Git worktree management
  - End-to-end test runner
  - Session logging with full API request/response JSON
- 212 unit and integration tests

### Fixed
- Path sandboxing escapes blocked
- Command blocking for dangerous patterns (rm -rf /, sudo, etc.)
- Output truncation at 50KB
- Conversation truncation (keep last 8 rounds)
- Tool pairing validation

### Security
- Run in sandboxed or disposable workspace
- Never expose API keys in public