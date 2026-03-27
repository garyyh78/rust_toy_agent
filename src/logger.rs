//! logger.rs - Colored stderr logging helpers
//!
//! All diagnostic output goes to stderr so stdout stays clean for the REPL.
//! Each function produces a different visual level:
//!
//! ┌──────────────────────────────────────────────────────────────┐
//! │ log_section  ───────────────────────────────────────────     │
//! │ Blue header banner separating major phases                   │
//! └──────────────────────────────────────────────────────────────┘
//!    log_info       label          value
//!    log_step       →              detail
//!    log_output_preview  ─  first 5 lines of output
//!
//! Function call graph:
//!
//!   agent_loop ──→ log_section("Round N")
//!       │
//!       ├──→ log_info("history", ...)
//!       ├──→ log_info("model", ...)
//!       ├──→ log_info("tokens", ...)
//!       ├──→ log_info("stop", ...)
//!       │
//!       ├──→ log_step("→", "Calling API...")
//!       ├──→ log_step("[1]", "bash: ...")
//!       ├──→ log_step("⚠", "Injecting nag reminder")
//!       │
//!       └──→ log_output_preview(output)
//!                └── shows first 5 lines, then "... (N more)"
//!
//! main() ──→ log_info("model", ...)
//!        ──→ log_info("workdir", ...)

/// Blue bordered section header for major loop phases.
pub fn log_section(title: &str) {
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    eprintln!("\x1b[34m {}\x1b[0m", title);
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
}

/// Cyan label-value pair for key-value diagnostic info.
pub fn log_info(label: &str, value: &str) {
    eprintln!("\x1b[36m  {:<12}\x1b[0m {}", label, value);
}

/// Yellow step marker for action descriptions within a section.
pub fn log_step(step: &str, detail: &str) {
    eprintln!("\x1b[33m  {}\x1b[0m {}", step, detail);
}

/// Gray preview of tool output: first 5 lines, then a truncation note.
pub fn log_output_preview(output: &str) {
    let lines: Vec<&str> = output.lines().take(5).collect();
    let truncated = output.lines().count() > 5;
    for line in &lines {
        eprintln!("\x1b[90m    {}\x1b[0m", line);
    }
    if truncated {
        eprintln!(
            "\x1b[90m    ... ({} more lines)\x1b[0m",
            output.lines().count() - 5
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // log functions write to stderr and return ().
    // We verify they don't panic; output is visual only.

    #[test]
    fn test_log_section_runs() {
        log_section("Test Section");
    }

    #[test]
    fn test_log_info_runs() {
        log_info("model", "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_log_step_runs() {
        log_step("→", "Doing something");
    }

    #[test]
    fn test_log_output_preview_short() {
        log_output_preview("line1\nline2");
    }

    #[test]
    fn test_log_output_preview_long() {
        let long = (1..=10)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        log_output_preview(&long);
    }
}
