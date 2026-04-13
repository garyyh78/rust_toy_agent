//! Constants for the binary (re-exports from config)

/// Canonical identity string for the top-level agent.
/// Used as both the default recipient for teammate messages
/// and the default owner field on tasks.
pub const LEAD: &str = "lead";

pub use crate::config::{
    IDLE_TIMEOUT_SECS as IDLE_TIMEOUT, MAX_ROUNDS, NAG_THRESHOLD,
    POLL_INTERVAL_SECS as POLL_INTERVAL, TEAMMATE_MAX_ROUNDS, TEAMMATE_MAX_TOKENS, TOKEN_THRESHOLD,
    TRUNCATE_ROUNDS,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lead_constant_is_lowercase() {
        assert_eq!(LEAD, "lead");
    }
}
