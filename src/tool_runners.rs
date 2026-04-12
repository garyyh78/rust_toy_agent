//! tool_runners.rs - Path helpers and file/shell tool runners
//!
//! Everything that touches the filesystem or runs shell commands lives here.
//!
//! ┌──────────────────────────────────────────────────────────┐
//! │                    tool_runners                          │
//! ├──────────────────────────────────────────────────────────┤
//! │                                                          │
//! │  Path helpers:                                           │
//! │  ┌──────────────┐    ┌──────────────┐                    │
//! │  │normalize_path│───→│  safe_path   │                    │
//! │  └──────────────┘    └──────┬───────┘                    │
//! │   resolves . and ..        │ rejects paths that          │
//! │                            │ escape workdir              │
//! │                            │                             │
//! │  Tool runners (all call safe_path first):                │
//! │                            │                             │
//! │            ┌───────────────┼───────────────┐             │
//! │            │               │               │             │
//! │     ┌──────▼──────┐ ┌─────▼──────┐ ┌──────▼──────┐      │
//! │     │  run_bash   │ │  run_read  │ │ run_write   │      │
//! │     └──────┬──────┘ └─────┬──────┘ └──────┬──────┘      │
//! │            │              │               │              │
//! │     sh -c command   read file N    write + mkdir         │
//! │     block dangerous  lines, cap     parent dirs          │
//! │     50KB cap         50KB cap       50KB cap             │
//! │                                                          │
//! │     ┌──────▼──────┐                                      │
//! │     │  run_edit   │                                      │
//! │     └─────────────┘                                      │
//! │      replacen(old, new, 1)                                │
//! └──────────────────────────────────────────────────────────┘

use std::path::{Component, Path, PathBuf};
use std::process::Command as Proc;

/// Maximum output size for tool results (50KB).
const MAX_OUTPUT_SIZE: usize = 50_000;

// -- Path helpers --
// Normalize resolves "." and ".." without touching the filesystem.
// safe_path joins a user-supplied path to the workdir and rejects
// any traversal that would escape the workspace root.

pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components: Vec<Component> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                components.pop();
            }
            c => components.push(c),
        }
    }
    components.iter().collect()
}

/// Resolve `p` relative to `workdir`, rejecting paths that escape the workspace.
pub fn safe_path(p: &str, workdir: &Path) -> Result<PathBuf, String> {
    let workdir_abs = workdir
        .canonicalize()
        .unwrap_or_else(|_| workdir.to_path_buf());
    let raw = workdir_abs.join(p);
    let normalized = normalize_path(&raw);
    if normalized.starts_with(&workdir_abs) {
        Ok(normalized)
    } else {
        Err(format!("Path escapes workspace: {p}"))
    }
}

// -- Tool runners --
// Each runner takes a workdir, calls safe_path when needed, and returns
// a string result. Errors are returned as strings, never panicking.

/// Run a shell command via `sh -c`. The substring blocklist below is a
/// best-effort guard against obvious footguns (`rm -rf /`, `sudo`, ...), NOT
/// a security boundary — it is trivially bypassed by piping, env tricks, or
/// alternative paths. Real isolation comes from running the agent inside a
/// sandbox/VM and keeping `workdir` outside sensitive trees.
pub fn run_bash(command: &str, workdir: &Path) -> String {
    let blocked = ["rm -rf /", "sudo", "shutdown", "reboot", "> /dev/"];
    if blocked.iter().any(|b| command.contains(b)) {
        return "Error: Dangerous command blocked".to_string();
    }
    match Proc::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(workdir)
        .output()
    {
        Err(e) => format!("Error: {e}"),
        Ok(out) => {
            // Merge stdout + stderr, trim, and cap at 50KB
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let text = text.trim().to_string();
            if text.is_empty() {
                "(no output)".to_string()
            } else if text.len() > MAX_OUTPUT_SIZE {
                text[..MAX_OUTPUT_SIZE].to_string()
            } else {
                text
            }
        }
    }
}

/// Read a file as UTF-8 text. Optionally limit to first N lines.
pub fn run_read(path: &str, limit: Option<usize>, workdir: &Path) -> String {
    match safe_path(path, workdir) {
        Err(e) => format!("Error: {e}"),
        Ok(fp) => match std::fs::read_to_string(&fp) {
            Err(e) => format!("Error: {e}"),
            Ok(text) => {
                let lines: Vec<&str> = text.lines().collect();
                let result: String = match limit {
                    Some(n) if n < lines.len() => {
                        let mut v: Vec<String> = lines[..n].iter().map(|s| s.to_string()).collect();
                        v.push(format!("... ({} more lines)", lines.len() - n));
                        v.join("\n")
                    }
                    _ => lines.join("\n"),
                };
                if result.len() > MAX_OUTPUT_SIZE {
                    result[..MAX_OUTPUT_SIZE].to_string()
                } else {
                    result
                }
            }
        },
    }
}

/// Write content to a file, creating parent directories as needed.
pub fn run_write(path: &str, content: &str, workdir: &Path) -> String {
    match safe_path(path, workdir) {
        Err(e) => format!("Error: {e}"),
        Ok(fp) => {
            if let Some(parent) = fp.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::write(&fp, content) {
                Ok(_) => format!("Wrote {} bytes to {}", content.len(), path),
                Err(e) => format!("Error: {e}"),
            }
        }
    }
}

/// Replace the first occurrence of `old_text` with `new_text` in a file.
pub fn run_edit(path: &str, old_text: &str, new_text: &str, workdir: &Path) -> String {
    match safe_path(path, workdir) {
        Err(e) => format!("Error: {e}"),
        Ok(fp) => match std::fs::read_to_string(&fp) {
            Err(e) => format!("Error: {e}"),
            Ok(content) => {
                if !content.contains(old_text) {
                    return format!("Error: Text not found in {path}");
                }
                let new_content = content.replacen(old_text, new_text, 1);
                match std::fs::write(&fp, new_content) {
                    Ok(_) => format!("Edited {path}"),
                    Err(e) => format!("Error: {e}"),
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- normalize_path --

    #[test]
    fn test_normalize_curdir() {
        let p = Path::new("a/./b");
        assert_eq!(normalize_path(p), PathBuf::from("a/b"));
    }

    #[test]
    fn test_normalize_parentdir() {
        let p = Path::new("a/b/../c");
        assert_eq!(normalize_path(p), PathBuf::from("a/c"));
    }

    #[test]
    fn test_normalize_multiple_parents() {
        let p = Path::new("a/b/../../c");
        assert_eq!(normalize_path(p), PathBuf::from("c"));
    }

    #[test]
    fn test_normalize_no_change() {
        let p = Path::new("a/b/c");
        assert_eq!(normalize_path(p), PathBuf::from("a/b/c"));
    }

    // -- safe_path --

    #[test]
    fn test_safe_path_allowed() {
        let workdir = std::env::current_dir().unwrap();
        let result = safe_path("src/main.rs", &workdir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_safe_path_escape_rejected() {
        let workdir = std::env::current_dir().unwrap();
        let result = safe_path("../../etc/passwd", &workdir);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("escapes workspace"));
    }

    // -- run_bash --

    #[test]
    fn test_run_bash_simple_echo() {
        let out = run_bash("echo hello", &PathBuf::from("."));
        assert!(out.contains("hello"));
    }

    #[test]
    fn test_run_bash_no_output() {
        let out = run_bash("true", &PathBuf::from("."));
        assert_eq!(out, "(no output)");
    }

    #[test]
    fn test_run_bash_captures_stderr() {
        let out = run_bash("ls /nonexistent 2>&1", &PathBuf::from("."));
        assert!(out.contains("nonexistent"));
    }

    #[test]
    fn test_run_bash_dangerous_blocked() {
        let dangerous = vec![
            "rm -rf /",
            "sudo rm -rf /tmp/foo",
            "shutdown now",
            "reboot",
            "cat /etc/passwd > /dev/null",
        ];
        for cmd in dangerous {
            let out = run_bash(cmd, &PathBuf::from("."));
            assert!(
                out.contains("Dangerous command blocked"),
                "Expected block for: {cmd}"
            );
        }
    }

    // -- run_read / run_write / run_edit --

    #[test]
    fn test_run_write_and_read() {
        let tmp = std::env::temp_dir().join("rust_toy_agent_test_write_read.txt");
        let path = tmp.to_str().unwrap();
        let workdir = PathBuf::from("/");

        let result = run_write(path, "line1\nline2\nline3", &workdir);
        assert!(result.contains("Wrote"));

        let content = run_read(path, None, &workdir);
        assert!(content.contains("line2"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_run_read_with_limit() {
        let tmp = std::env::temp_dir().join("rust_toy_agent_test_limit.txt");
        let path = tmp.to_str().unwrap();
        let workdir = PathBuf::from("/");

        let _ = std::fs::write(&tmp, "a\nb\nc\nd\ne");
        let content = run_read(path, Some(2), &workdir);
        assert!(content.contains("a"));
        assert!(content.contains("b"));
        assert!(content.contains("more lines"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_run_edit() {
        let tmp = std::env::temp_dir().join("rust_toy_agent_test_edit.txt");
        let path = tmp.to_str().unwrap();
        let workdir = PathBuf::from("/");

        let _ = std::fs::write(&tmp, "hello world");
        let result = run_edit(path, "world", "rust", &workdir);
        assert!(result.contains("Edited"));

        let content = std::fs::read_to_string(&tmp).unwrap();
        assert_eq!(content, "hello rust");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_run_edit_text_not_found() {
        let tmp = std::env::temp_dir().join("rust_toy_agent_test_edit_nf.txt");
        let path = tmp.to_str().unwrap();
        let workdir = PathBuf::from("/");

        let _ = std::fs::write(&tmp, "hello world");
        let result = run_edit(path, "missing", "replaced", &workdir);
        assert!(result.contains("Text not found"));
        let _ = std::fs::remove_file(&tmp);
    }
}
