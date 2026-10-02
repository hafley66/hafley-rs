#!/bin/bash
# Fresh hafley-rxjs dogfood corpus at d0802620 for one caller. Usage: 0_corpus.sh <dir>
# Copies the .dogfood harness (per-package tsconfigs mapping @hafley66/* to src, tsrefs.cjs, baselines).
set -euo pipefail
dir=${1:?corpus dir}
src=/Users/chrishafley/projects/hafley-rxjs
harness=/Users/chrishafley/projects/hafley-rxjs-ryi-dogfood/.dogfood
if [ ! -d "$dir/.git" ] && [ ! -f "$dir/.git" ]; then
  git -C "$src" worktree add --detach "$dir" d0802620 >/dev/null
fi
git -C "$dir" reset -q --hard d0802620
git -C "$dir" clean -qfd -e .dogfood
mkdir -p "$dir/.dogfood"
cp -n "$harness"/tsconfig.*.json "$harness"/tsrefs.cjs "$harness"/base.*.txt "$dir/.dogfood/" 2>/dev/null || true
[ -d "$dir/node_modules" ] || ln -s "$src/node_modules" "$dir/node_modules"
echo "$dir"
