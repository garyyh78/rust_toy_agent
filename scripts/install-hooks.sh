#!/usr/bin/env bash
set -e
cd "$(dirname "$0")/.."
mkdir -p .git/hooks
ln -sf ../../scripts/git-hooks/pre-commit .git/hooks/pre-commit
echo "Installed pre-commit hook"