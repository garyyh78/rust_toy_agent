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

const BASH_ENV_ALLOWLIST: &[&str] = &[
    "PATH", "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "TERM", "TMPDIR", "SHELL", "PWD",
];

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

fn canonicalize_partial(p: &Path) -> Result<PathBuf, String> {
    let mut existing = p.to_path_buf();
    let mut suffix = PathBuf::new();
    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            let name = name.to_owned();
            existing.pop();
            if suffix.as_os_str().is_empty() {
                suffix = PathBuf::from(name);
            } else {
                let mut new_suffix = PathBuf::from(name);
                new_suffix.push(&suffix);
                suffix = new_suffix;
            }
        } else {
            return Err("root has no name".to_string());
        }
    }
    let canon = existing
        .canonicalize()
        .map_err(|e| format!("canon: {}", e))?;
    let mut result = canon;
    if !suffix.as_os_str().is_empty() {
        result.push(&suffix);
    }
    Ok(result)
}

pub fn safe_path(p: &str, workdir: &Path) -> Result<PathBuf, String> {
    let workdir_canon = workdir
        .canonicalize()
        .map_err(|e| format!("workdir canon: {}", e))?;
    let joined = workdir.join(p);
    let resolved = canonicalize_partial(&joined)?;
    if !resolved.starts_with(&workdir_canon) {
        return Err(format!("path escapes sandbox: {}", p));
    }
    Ok(resolved)
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
    let mut cmd = Proc::new("sh");
    cmd.arg("-c").arg(command);
    cmd.current_dir(workdir);
    cmd.env_clear();
    for key in BASH_ENV_ALLOWLIST {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }
    match cmd.output() {
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
        assert!(result.unwrap_err().contains("escapes sandbox"));
    }

    #[test]
    #[cfg(unix)]
    fn symlink_outside_workdir_is_rejected() {
        let tmp = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        std::fs::write(outside.path().join("secret"), "x").unwrap();
        let link = tmp.path().join("escape");
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        let bad = safe_path("escape/secret", tmp.path());
        assert!(bad.is_err(), "symlink escape was accepted: {:?}", bad);
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

    #[test]
    fn bash_does_not_inherit_api_key() {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-secret-test");
        let out = run_bash("echo $ANTHROPIC_API_KEY", &PathBuf::from("."));
        assert!(!out.contains("sk-secret-test"), "API key leaked: {out}");
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    // -- run_read / run_write / run_edit --

    #[test]
    fn test_run_write_and_read() {
        let tmp = tempfile::TempDir::new().unwrap();
        let workdir = tmp.path().to_path_buf();
        let filename = "test_write_read.txt";

        let result = run_write(filename, "line1\nline2\nline3", &workdir);
        assert!(result.contains("Wrote"));

        let content = run_read(filename, None, &workdir);
        assert!(content.contains("line2"));
    }

    #[test]
    fn test_run_read_with_limit() {
        let tmp = tempfile::TempDir::new().unwrap();
        let workdir = tmp.path().to_path_buf();
        let filename = "test_limit.txt";

        run_write(filename, "a\nb\nc\nd\ne", &workdir);
        let content = run_read(filename, Some(2), &workdir);
        assert!(content.contains("a"));
        assert!(content.contains("b"));
        assert!(content.contains("more lines"));
    }

    #[test]
    fn test_run_edit() {
        let tmp = tempfile::TempDir::new().unwrap();
        let workdir = tmp.path().to_path_buf();
        let filename = "test_edit.txt";

        run_write(filename, "hello world", &workdir);
        let result = run_edit(filename, "world", "rust", &workdir);
        assert!(result.contains("Edited"));

        let content = run_read(filename, None, &workdir);
        assert!(content.contains("hello rust"));
    }

    #[test]
    fn test_run_edit_text_not_found() {
        let tmp = tempfile::TempDir::new().unwrap();
        let workdir = tmp.path().to_path_buf();
        let filename = "test_edit_nf.txt";

        run_write(filename, "hello world", &workdir);
        let result = run_edit(filename, "missing", "replaced", &workdir);
        assert!(result.contains("Text not found"));
    }
}
