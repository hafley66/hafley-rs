#!/usr/bin/env bash

# Topic status sweep: git head, matching issues, epics, briefs, latest chat log.
# Usage: scripts/where-were-we.sh <topic-regex> [more-regex ...]
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
PAT="${1:?usage: where-were-we.sh <topic-regex> [...]}"
shift
for extra in "$@"; do PAT="$PAT|$extra"; done

step() { printf '\n\033[1m== %s\033[0m\n' "$1"; }

step "head"
git log --oneline -1
git status --short --branch | head -1

step "commits matching /$PAT/ (last 200)"
git log --oneline -200 --extended-regexp --regexp-ignore-case --grep="$PAT" | head -15

step "branches matching /$PAT/"
git branch -a --sort=-committerdate | grep -iE "$PAT" | head -10 || echo "(none)"

step "issues matching /$PAT/"
for d in issues/*/; do
  f="$d/item.md"
  [ -f "$f" ] || continue
  name=$(basename "$d")
  if echo "$name" | grep -qiE "$PAT" || grep -qiE "^(epic|labels):.*($PAT)" "$f"; then
    st=$(sed -n 's/^status: *//p' "$f" | head -1)
    ty=$(sed -n 's/^type: *//p' "$f" | head -1)
    up=$(sed -n 's/^updated: *//p' "$f" | head -1)
    printf '%-10s %-8s %-12s %s\n' "${st:-?}" "${ty:-?}" "${up:-?}" "$name"
  fi
done | sort

step "epic acceptance"
for f in issues/*/item.md; do
  [ "$(sed -n 's/^type: *//p' "$f" | head -1)" = epic ] || continue
  basename "$(dirname "$f")" | grep -qiE "$PAT" || grep -qiE "^labels:.*($PAT)" "$f" || continue
  echo "-- $(basename "$(dirname "$f")")"
  grep -E '^- \[[ x]\]' "$f" | head -10
done

step "plans / briefs / TASKS matching /$PAT/"
ls -t plans/**/*.md plans/*.md TASKS/*.md docs/**/*.md 2>/dev/null \
  | grep -iE "$PAT" | head -10 || echo "(none)"

step "latest chat log"
tail -1 chat_log/LATEST.md
