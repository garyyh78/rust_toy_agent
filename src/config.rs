//! config.rs - Centralized tuning knobs for the agent library.
//!
//! Every numerical cap lives here so they can be audited in one place.

/// Response cap for the lead agent loop. Sized for multi-tool
/// rounds with long tool outputs.
pub const LEAD_MAX_TOKENS: u32 = 16 * 1024;

/// Subagents do focused, short work — smaller cap limits
/// runaway children.
pub const SUBAGENT_MAX_TOKENS: u32 = 8_000;

/// Compactor rewrites history in one shot; size to comfortably
/// hold ~2× the incoming buffer.
pub const COMPACTOR_MAX_TOKENS: u32 = 8_000;

/// Summarization uses less tokens since it's just a summary.
pub const SUMMARIZE_MAX_TOKENS: u32 = 2_000;

/// Max rounds of a single `agent_loop` before forced stop.
pub const MAX_ROUNDS: u32 = 50;

/// Token threshold for triggering auto-compaction.
pub const TOKEN_THRESHOLD: usize = 100_000;

/// Token threshold for triggering micro-compaction (smaller, more aggressive).
pub const COMPACT_THRESHOLD: usize = 50_000;

/// Teammate agents follow subagent conventions.
pub const TEAMMATE_MAX_TOKENS: u32 = 8_000;

/// Teammate agents follow subagent conventions.
pub const TEAMMATE_MAX_ROUNDS: u32 = 30;

/// Skill-loading helper agent.
pub const SKILL_MAX_TOKENS: u32 = 8_000;

/// Nag threshold: how many rounds without a todo touch
/// before we inject a reminder.
pub const NAG_THRESHOLD: usize = 3;

/// Truncation window: keep the last N rounds.
pub const TRUNCATE_ROUNDS: usize = 8;

/// Inbox poll cadence (seconds).
pub const POLL_INTERVAL_SECS: u64 = 5;

/// Idle timeout (seconds).
pub const IDLE_TIMEOUT_SECS: u64 = 60;

/// Bash command timeout (seconds).
pub const BASH_TIMEOUT_SECS: u64 = 60;

/// Maximum output size for tool results (50KB).
pub const MAX_TOOL_OUTPUT_BYTES: usize = 50_000;

/// Allowlist of environment variables passed to bash commands.
pub const BASH_ENV_ALLOWLIST: &[&str] = &[
    "PATH", "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "TERM", "TMPDIR", "SHELL", "PWD",
];
