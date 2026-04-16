# Security Model

## Overview

This project executes arbitrary shell commands. Treat it accordingly.

## Threat Model

### In Scope

- Shell command injection via LLM-generated commands
- API key exposure
- File system access beyond workspace
- Long-running processes

### Out of Scope

- Social engineering
- Physical access
- Third-party services beyond API provider

## Mitigations

### Path Sandboxing

`safe_path()` rejects paths that escape the workspace:

```
# Blocked:
/etc/passwd
../../etc/passwd
$HOME/.ssh/id_rsa
```

### Command Blocklist

These patterns are blocked in `bash`:

- `rm -rf /`, `rm -rf ~`
- `sudo`, `su`
- `shutdown`, `reboot`
- `> /dev/null` (output suppression)

### Output Limits

Tool output truncated at **50KB**.

### Conversation Truncation

Last **8 rounds** kept to prevent API overflow.

## Recommendations

1. **Run in a sandbox**: Disposable VM, container, or isolated directory
2. **Separate API keys**: Use dedicated keys for agent work
3. **Review commands**: Before running agent-output commands
4. **Limit permissions**: Agent should only access what's needed
5. **Monitor logs**: Check `logs/` for activity

## Reporting

See [SECURITY.md](../SECURITY.md)