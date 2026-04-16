#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="$SCRIPT_DIR/swe_bench_data"
mkdir -p "$DATA_DIR"

REPOS=(
    "django__django-12113:https://github.com/django/django.git"
    "marshmallow-code__marshmallow-1359:https://github.com/marshmallow-code/marshmallow.git"
    "sympy__sympy-20590:https://github.com/sympy/sympy.git"
)

for repo_spec in "${REPOS[@]}"; do
    name="${repo_spec%%:*}"
    url="${repo_spec##*:}"
    target_dir="$DATA_DIR/$name/repo"

    if [ -d "$target_dir" ]; then
        echo "Skipping $name (already exists)"
    else
        echo "Cloning $name..."
        mkdir -p "$DATA_DIR/$name"
        git clone --depth 1 "$url" "$target_dir"
    fi
done

echo "Done. SWE-bench data fetched to $DATA_DIR/"