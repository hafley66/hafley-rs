#!/bin/bash
set -euo pipefail
before=$(git status --porcelain)
for tier in fast slow; do
  for drag in no yes; do
    flags=(); [ "$tier" = fast ] || flags+=(--slow); [ "$drag" = no ] || flags+=(--drag)
    if "$RYII" cleave packages/md/src/ports.ts#installMdviewHost packages/md/src/0_hostInstall.ts --state "$STATE/$tier-$drag" --commit "${flags[@]}" > ".dogfood/D07.$tier-$drag" 2>&1; then
      echo 'D7: unsafe mutable-binding cleave unexpectedly succeeded'; exit 1
    fi
    rg 'refuses mutable binding host' ".dogfood/D07.$tier-$drag"
    [ ! -e packages/md/src/0_hostInstall.ts ]
    [ "$before" = "$(git status --porcelain)" ]
  done
done
echo 'D7: fast/slow cleave refuses imported mutable host with and without drag'
