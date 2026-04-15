#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
HOOK_SOURCE="$REPO_ROOT/scripts/git-hooks/pre-commit"
HOOK_TARGET="$REPO_ROOT/.git/hooks/pre-commit"

if [[ ! -f "$HOOK_SOURCE" ]]; then
    echo "Error: pre-commit hook not found at $HOOK_SOURCE" >&2
    exit 1
fi

mkdir -p "$REPO_ROOT/.git/hooks"
ln -sf "$HOOK_SOURCE" "$HOOK_TARGET"
echo "Installed pre-commit hook to $HOOK_TARGET"