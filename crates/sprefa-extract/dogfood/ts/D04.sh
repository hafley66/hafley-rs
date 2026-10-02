#!/bin/bash
set -euo pipefail
CASE=D04
source "$(dirname "${BASH_SOURCE[0]}")/1_type_errors.sh"
type_baseline boop-xterm
src=packages/boop-xterm/src/8i_turnPanel.ts
dest=packages/boop-xterm/src/panels/8i_turnPanel.ts
"$RYII" move "$src" "$dest" --state "$STATE" --commit > .dogfood/D04.commit
for file in packages/boop-xterm/src/9_view.ts packages/boop-xterm/src/8j_agentSquares.browser.test.ts; do
  rg 'import\("\./panels/8i_turnPanel\.js"\)' "$file"
  ! rg 'import\("\./8i_turnPanel\.js"\)' "$file"
done
src=packages/vitest-telemetry/src/adapter/navTree.ts
[ -f "$src" ] || src=$(rg --files packages/vitest-telemetry | rg '/adapter/navTree.ts$')
dest=${src%/adapter/navTree.ts}/nav/navTree.ts
"$RYII" move "$src" "$dest" --state "$STATE" --commit >> .dogfood/D04.commit
model=${src%/adapter/navTree.ts}/model.test.ts
rg "typeof import\('./nav/navTree.js'\)" "$model"
! rg "typeof import\('./adapter/navTree.js'\)" "$model"
type_no_new boop-xterm
echo 'D4: import types and typeof import specifiers follow moved files'
