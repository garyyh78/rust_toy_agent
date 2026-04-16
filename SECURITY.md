# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

This project executes arbitrary shell commands and handles API keys. Security issues should be reported responsibly.

### How to Report

1. **Do NOT** open a public GitHub issue for security vulnerabilities
2. Email security concerns directly to the maintainer
3. Include the following in your report:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Any fixes you suggest (optional)

### Scope

This security policy covers:
- The rust_toy_agent CLI tool
- Any tools or plugins distributed with this repository
- The execution of shell commands via the `bash` tool

### Out of Scope

- Social engineering attacks
- Physical security
- Third-party services not controlled by this project

### Response Timeline

- Acknowledgment: Within 48 hours
- Initial assessment: Within 7 days
- Fix timeline: Depends on severity

## Security Best Practices

- Run in a sandboxed or disposable workspace
- Never expose `ANTHROPIC_API_KEY` in public repositories
- Review tool permissions before running agent-generated commands
- Use separate API keys for agent work if possible