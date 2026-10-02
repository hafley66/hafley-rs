#!/bin/bash
# Fresh hafley-rxjs corpus at d0802620 and read-only hafley-tsp at 8f679b1.
# Usage: 0_corpus.sh <dir>; TypeSpec sources live beside it at <dir>.tsp.
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
tsp_src=/Users/chrishafley/projects/hafley-tsp
tsp_dir="${dir}.tsp"
if [ ! -d "$tsp_dir/.git" ] && [ ! -f "$tsp_dir/.git" ]; then
  git -C "$tsp_src" worktree add --detach "$tsp_dir" 8f679b1 >/dev/null
  chmod -R a-w "$tsp_dir"
fi
# Reuse only the pinned, untouched checkout; never reset or clean this root.
[ "$(git -C "$tsp_dir" rev-parse HEAD)" = "$(git -C "$tsp_src" rev-parse 8f679b1)" ]
[ -z "$(git -C "$tsp_dir" status --porcelain)" ]
echo "$dir"
