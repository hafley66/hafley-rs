#!/bin/bash
set -euo pipefail
# A named barrel alongside the corpus glob barrel isolates re-export repair
# from the separate D7 refusal for installMdviewHost's assigned host binding.
cat > packages/signals/src/2b_isSignalBarrel.ts <<'TS'
export { isSignal, toSignal } from './2_Signal.js'
TS
"$RYII" cleave packages/signals/src/2_Signal.ts#isSignal packages/signals/src/2a_isSignal.ts --state "$STATE" --commit > .dogfood/D06.commit
rg "export.*isSignal.*2a_isSignal" packages/signals/src/2b_isSignalBarrel.ts
rg "export.*toSignal.*2_Signal" packages/signals/src/2b_isSignalBarrel.ts
! rg "import.*isSignal" packages/signals/src/2b_isSignalBarrel.ts
echo 'D6: named re-export moves its item and keeps the remaining public exports'
