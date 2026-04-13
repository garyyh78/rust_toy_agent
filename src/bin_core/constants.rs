//! Constants for the binary

/// Token threshold for triggering auto-compaction.
pub const TOKEN_THRESHOLD: usize = 100_000;

/// Poll interval in seconds for teammate idle checks.
pub const POLL_INTERVAL: u64 = 5;

/// Idle timeout in seconds before teammate shuts down.
pub const IDLE_TIMEOUT: u64 = 60;

/// Maximum tokens for LLM responses in the main agent loop.
pub const MAX_TOKENS: u32 = 8_000;

/// Nag threshold: inject reminder after N rounds without todo update.
pub const NAG_THRESHOLD: usize = 3;

/// Truncate history to this many rounds.
pub const TRUNCATE_ROUNDS: usize = 8;

/// Maximum rounds for main agent loop.
pub const MAX_ROUNDS: u32 = 50;

/// Maximum rounds for teammate work phase.
pub const TEAMMATE_MAX_ROUNDS: u32 = 30;

/// Maximum tokens for teammate LLM calls.
pub const TEAMMATE_MAX_TOKENS: u32 = 8_000;
