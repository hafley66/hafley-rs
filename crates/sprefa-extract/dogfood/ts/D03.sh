#!/bin/bash
set -euo pipefail
before=$(shasum packages/md/package.json)
"$RYII" move packages/md/src/ports.ts packages/md/src/0_ports.ts --state "$STATE" > .dogfood/D03.plan
! rg '^dep ' .dogfood/D03.plan
"$RYII" move packages/md/src/ports.ts packages/md/src/0_ports.ts --state "$STATE" --commit > .dogfood/D03.commit
[ "$before" = "$(shasum packages/md/package.json)" ]
echo 'D3: in-package move leaves package dependencies unchanged'
