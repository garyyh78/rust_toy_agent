# Configuration

## Environment Variables

Create a `.env` file from the template:

```bash
cp .env.example .env
```

### Required

| Variable | Description | Default |
|----------|-------------|--------|
| `ANTHROPIC_API_KEY` | API key for Anthropic (or compatible) | - |
| `MODEL_ID` | Model to use | `claude-sonnet-4-20250514` |

### Optional

| Variable | Description | Default |
|----------|-------------|--------|
| `ANTHROPIC_BASE_URL` | API endpoint | `https://api.anthropic.com` |
| `ANTHROPIC_API_VERSION` | API version | `2023-06-01` |

### Example .env

```
ANTHROPIC_API_KEY=sk_ant-api03-...
MODEL_ID=claude-sonnet-4-20250514
```

## MSRV

Minimum Supported Rust Version: **1.75**

The project is tested in CI against 1.75 and stable.

## OS Support

Tested on:
- macOS (Intel, Apple Silicon)
- Linux (Ubuntu 22.04+)
- FreeBSD

Not currently tested on Windows (WSL recommended).