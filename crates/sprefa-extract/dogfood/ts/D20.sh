#!/bin/bash
set -euo pipefail
src=packages/vitest-telemetry/src/report-app/adapter/navTree.ts
base=packages/vitest-telemetry/src/report-app
before=$(git status --porcelain)
mkdir -p "$base/0_preexistingEmpty"
for dest in "$base/nav/0_nested/navTree.ts" "$base/0_preexistingEmpty/navTree.ts"; do
  if "$RYII" move "$src" "$dest" --state "$STATE" --commit --verify 'exit 1' > .dogfood/D20.verify 2>&1; then
    echo 'D20: failing verifier unexpectedly succeeded'; exit 1
  else rc=$?; fi
  [ "$rc" -eq 3 ]
  [ -f "$src" ] && [ ! -e "$dest" ]
  [ ! -d "$base/nav" ]
  [ -d "$base/0_preexistingEmpty" ]
  [ "$before" = "$(git status --porcelain)" ]
done
rmdir "$base/0_preexistingEmpty"
echo 'D20: rollback restores files, removes created ancestors, retains existing empty dirs'
