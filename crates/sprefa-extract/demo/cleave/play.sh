#!/usr/bin/env bash
# @comment-ok: module header, the seam list every demo script opens with
# Four `ryi cleave` moves over a six-file TypeScript app, each committed so
# `git diff --stat HEAD~1` shows exactly what the verb wrote. The app is
# copied into a fresh mktemp -d; the checkout is never written.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ryi="${RYI:-ryi}"
if ! command -v "$ryi" >/dev/null 2>&1; then
  echo "play: no ryi on PATH; set RYI to the binary" >&2
  exit 2
fi

play="$(mktemp -d "${TMPDIR:-/tmp}/ryi-cleave-play.XXXXXX")"
repo="$play/repo"
state="$play/state"
mkdir -p "$repo" "$state"
cp -R "$here/src" "$repo/src"
cd "$repo"
git init -q .
git add -A
git -c user.email=cleave@demo -c user.name=cleave-demo commit -qm "the app before any cleave"

steps=0
play_step() {
  local label="$1"
  shift
  steps=$((steps + 1))
  echo
  echo "== step $steps: $label"
  echo "-- ryi cleave $*"
  HAFLEY_TRACE="$play/trace-$steps.json" timeout 10 "$ryi" cleave "$@" \
    --root "$repo" --state "$state" --commit
  git add -A
  git -c user.email=cleave@demo -c user.name=cleave-demo commit -qm "cleave $steps: $label"
  git log --oneline
  git diff --stat HEAD~1
}

# 1. Plain: report.ts imports `describe` alone, so its specifier is re-aimed
#    whole rather than split.
play_step "describe -> text.ts, a single-name importer re-aimed" \
  src/utils.ts#describe src/text.ts

# 2. Split: app.ts imports three names from ./utils, so the specifier is cut
#    and a second import line lands beside it.
play_step "loadConfig -> config.ts, an importer's specifier split" \
  src/utils.ts#loadConfig src/config.ts

# 3. Drag: `tidy` is private and only `slug` uses it, so it travels.
play_step "slug -> text.ts with --drag, the private helper tidy moves too" \
  src/utils.ts#slug src/text.ts --drag

# 4. New destination: src/banner.ts does not exist, and `SEP` is private and
#    shared, so it stays in utils.ts with an export and banner.ts imports it.
play_step "banner -> src/banner.ts, a created file importing an exported helper" \
  src/utils.ts#banner src/banner.ts

echo
echo "== check"
if command -v tsc >/dev/null 2>&1; then
  report="$(timeout 10 tsc --noEmit --target es2020 --module esnext \
    --moduleResolution bundler src/*.ts 2>&1 || true)"
  errors="$(printf '%s\n' "$report" | grep -c 'error TS' || true)"
  echo "check tsc --noEmit -> $errors errors"
  printf '%s\n' "$report"
  test "$errors" -eq 0
else
  rows="$(HAFLEY_TRACE="$play/trace-check.json" timeout 10 "$ryi" fast src/*.ts | wc -l | tr -d ' ')"
  echo "check ryi fast (tsc is not installed) -> $rows fact rows"
  test "$rows" -gt 0
fi

commits="$(git rev-list --count HEAD)"
echo
echo "play: $steps cleave steps, $commits commits, tree at $repo"
