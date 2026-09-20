#!/usr/bin/env bash
# Mechanism 1: `aider --show-repo-map` over a scratch git repo holding only the
# case's fixture files, at their crates/sprefa-extract-relative paths. A scratch
# repo keeps aider's .aider* litter and the whole-repo map out of the worktree.
# usage: map.sh <out-file> <fixture-file>...
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
extract="$here/../../../sprefa-extract"
out="$1"; shift
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
for f in "$@"; do
  mkdir -p "$scratch/$(dirname "$f")"
  cp "$extract/$f" "$scratch/$f"
done
git -C "$scratch" init -q
git -C "$scratch" add -A
git -C "$scratch" -c user.email=a@b -c user.name=aider commit -q -m scratch
(cd "$scratch" && OPENAI_API_KEY=unused "$here/.venv/bin/aider" \
  --show-repo-map --map-tokens 8192 --map-refresh manual \
  --no-gitignore --no-check-update --no-analytics --no-auto-commits \
  --no-show-model-warnings --no-pretty --no-stream --model gpt-4o \
  --chat-history-file /dev/null --input-history-file /dev/null </dev/null 2>&1) \
  | sed -n '/^Here are summaries/,$p' > "$out"
