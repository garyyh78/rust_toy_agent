//! logger.rs - Colored stderr logging with optional session file logging
//!
//! All diagnostic output goes to stderr so stdout stays clean for the REPL.
//! When a `SessionLogger` is used, the same output is also written to a
//! timestamped log file (without ANSI colors).
//!
//! ┌──────────────────────────────────────────────────────────────┐
//! │                      SessionLogger                           │
//! ├──────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │   new("logs/session.log")                                   │
//! │       │                                                     │
//! │       ├── writes to stderr (with colors)                    │
//! │       └── writes to file  (plain text, timestamped)         │
//! │                                                              │
//! │   log_section("Round 1")     → stderr + file                │
//! │   log_info("model", "claude") → stderr + file               │
//! │   log_step("→", "calling API") → stderr + file              │
//! │   log_output_preview(output)  → stderr (5 lines) + file     │
//! │                                                              │
//! │   log_user_input("list files")  → file only                 │
//! │   log_agent_response("done")    → file only                 │
//! │                                                              │
//! │   Free functions (no file):                                 │
//! │   ┌────────────┐  ┌──────────┐  ┌──────────┐               │
//! │   │log_section │  │log_info  │  │log_step  │               │
//! │   └────────────┘  └──────────┘  └──────────┘               │
//! │   ┌──────────────────┐                                      │
//! │   │log_output_preview│                                      │
//! │   └──────────────────┘                                      │
//! └──────────────────────────────────────────────────────────────┘

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

#[allow(dead_code)]
const MAX_LOG_FILES: usize = 20;

#[allow(dead_code)]
fn prune_old_logs(dir: &Path) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("session_"))
        .collect();
    entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    while entries.len() > MAX_LOG_FILES {
        let oldest = entries.remove(0);
        let _ = std::fs::remove_file(oldest.path());
    }
    Ok(())
}

// -- Free functions (stderr only, no file) --

pub fn log_section(title: &str) {
    eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    eprintln!(" {}", title);
    eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
}

pub fn log_info(label: &str, value: &str) {
    eprintln!("  {:<12} {}", label, value);
}

pub fn log_step(step: &str, detail: &str) {
    eprintln!("  {step} {detail}");
}

pub fn log_output_preview(output: &str) {
    let lines: Vec<&str> = output.lines().take(5).collect();
    let truncated = output.lines().count() > 5;
    for line in &lines {
        eprintln!("    {line}");
    }
    if truncated {
        eprintln!("    ... ({} more lines)", output.lines().count() - 5);
    }
}

// -- SessionLogger (stderr + file) --

/// Logs to both stderr (colored) and a plain-text session file.
pub struct SessionLogger {
    file: Option<File>,
}

impl SessionLogger {
    /// Create a logger that writes to stderr only (no file).
    pub fn stderr_only() -> Self {
        Self { file: None }
    }

    /// Open a log file at `path`, creating parent directories as needed.
    /// Also writes to stderr.
    pub fn new(path: &str) -> Result<Self, String> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create log dir: {e}"))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| format!("Failed to open log file {path}: {e}"))?;
        let _ = prune_old_logs(Path::new(path).parent().unwrap());
        Ok(Self { file: Some(file) })
    }

    /// Write a line to the log file (no-op if no file).
    fn write_file(&mut self, line: &str) {
        if let Some(ref mut f) = self.file {
            if let Err(e) = writeln!(f, "{line}") {
                tracing::error!(error = %e, "log file write failed");
            }
        }
    }

    /// Get current timestamp as HH:MM:SS in local timezone.
    fn timestamp() -> String {
        chrono::Local::now().format("%H:%M:%S").to_string()
    }

    pub fn log_section(&mut self, title: &str) {
        log_section(title);
        self.write_file(&format!("[{}] === {} ===", Self::timestamp(), title));
    }

    pub fn log_info(&mut self, label: &str, value: &str) {
        log_info(label, value);
        self.write_file(&format!(
            "[{}]   {:<12} {}",
            Self::timestamp(),
            label,
            value
        ));
    }

    pub fn log_step(&mut self, step: &str, detail: &str) {
        log_step(step, detail);
        self.write_file(&format!("[{}]   {step} {detail}", Self::timestamp()));
    }

    pub fn log_output_preview(&mut self, output: &str) {
        log_output_preview(output);
        // Write full output to file (not truncated)
        for line in output.lines() {
            self.write_file(&format!("[{}]     {line}", Self::timestamp()));
        }
    }

    /// Log user input to file only (not stderr, since it's already shown via prompt).
    pub fn log_user_input(&mut self, input: &str) {
        self.write_file(&format!("[{}] USER > {input}", Self::timestamp()));
    }

    /// Log final agent text response to file.
    pub fn log_agent_response(&mut self, text: &str) {
        for line in text.lines() {
            self.write_file(&format!("[{}] AGENT < {line}", Self::timestamp()));
        }
    }

    /// Log the full API request JSON to file.
    pub fn log_api_request(&mut self, body: &serde_json::Value) {
        self.write_file(&format!(
            "[{}] ── API REQUEST ──────────────────────────────",
            Self::timestamp()
        ));
        for line in serde_json::to_string_pretty(body)
            .unwrap_or_default()
            .lines()
        {
            self.write_file(&format!("[{}]   {line}", Self::timestamp()));
        }
    }

    /// Log the full API response JSON to file.
    pub fn log_api_response(&mut self, body: &serde_json::Value) {
        self.write_file(&format!(
            "[{}] ── API RESPONSE ─────────────────────────────",
            Self::timestamp()
        ));
        for line in serde_json::to_string_pretty(body)
            .unwrap_or_default()
            .lines()
        {
            self.write_file(&format!("[{}]   {line}", Self::timestamp()));
        }
    }

    /// Log an API error string to file.
    pub fn log_api_error(&mut self, error: &str) {
        self.write_file(&format!(
            "[{}] ── API ERROR ────────────────────────────────",
            Self::timestamp()
        ));
        self.write_file(&format!("[{}]   {error}", Self::timestamp()));
    }

    /// Log a session start marker.
    pub fn log_session_start(&mut self, model: &str, workdir: &str) {
        self.write_file("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        self.write_file(&format!(
            "[{}] SESSION START: model={model} workdir={workdir}",
            Self::timestamp()
        ));
        self.write_file("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    }

    /// Log a session end marker.
    pub fn log_session_end(&mut self) {
        self.write_file(&format!("[{}] SESSION END", Self::timestamp()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -- Free function tests --

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
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        log_output_preview(&long);
    }

    // -- SessionLogger tests --

    #[test]
    fn test_session_logger_stderr_only() {
        let mut logger = SessionLogger::stderr_only();
        logger.log_section("test");
        logger.log_info("key", "value");
        logger.log_step("→", "step");
        logger.log_output_preview("line1\nline2");
        logger.log_user_input("hello");
        logger.log_agent_response("response");
    }

    #[test]
    fn test_session_logger_creates_file() {
        let tmp_dir = TempDir::new().unwrap();
        let log_path = tmp_dir.path().join("session_test.log");
        let path = log_path.to_str().unwrap();

        let mut logger = SessionLogger::new(path).unwrap();
        logger.log_session_start("test-model", "/tmp");
        logger.log_section("Round 1");
        logger.log_info("model", "test");
        logger.log_step("→", "calling API");
        logger.log_output_preview("output line 1\noutput line 2");
        logger.log_user_input("list files");
        logger.log_agent_response("Here are the files.");
        logger.log_session_end();

        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("SESSION START"));
        assert!(content.contains("Round 1"));
        assert!(content.contains("USER > list files"));
        assert!(content.contains("AGENT < Here are the files."));
        assert!(content.contains("SESSION END"));
        assert!(content.contains("output line 1"));
        drop(tmp_dir);
    }

    #[test]
    fn test_session_logger_creates_parent_dirs() {
        let tmp_dir = TempDir::new().unwrap();
        let log_path = tmp_dir.path().join("nested/session.log");
        let path = log_path.to_str().unwrap();

        let mut logger = SessionLogger::new(path).unwrap();
        logger.log_info("test", "nested dir");

        assert!(log_path.exists());
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("nested dir"));
        drop(tmp_dir);
    }

    #[test]
    fn test_session_logger_timestamp_format() {
        let ts = SessionLogger::timestamp();
        // Should be HH:MM:SS (10 chars with colons)
        assert_eq!(ts.len(), 8);
        assert_eq!(ts.as_bytes()[2], b':');
        assert_eq!(ts.as_bytes()[5], b':');
    }

    #[test]
    fn test_session_logger_no_file_no_panic() {
        let mut logger = SessionLogger::stderr_only();
        // Should not panic even without a file
        logger.log_user_input("test");
        logger.log_agent_response("response");
        logger.log_session_start("m", "/d");
        logger.log_session_end();
    }

    #[test]
    fn test_prune_old_logs_keeps_20() {
        let tmp_dir = TempDir::new().unwrap();
        let prune_dir = tmp_dir.path().join("prune_test");
        std::fs::create_dir_all(&prune_dir).unwrap();

        for i in 0..25 {
            let path = prune_dir.join(format!("session_{:02}.log", i));
            std::fs::write(&path, format!("log {}", i)).unwrap();
        }

        std::thread::sleep(std::time::Duration::from_millis(10));

        prune_old_logs(&prune_dir).unwrap();

        let remaining: Vec<_> = std::fs::read_dir(&prune_dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("session_"))
            .collect();
        assert_eq!(remaining.len(), 20);

        drop(tmp_dir);
    }
}
