#!/bin/bash
set -euo pipefail
for tier in fast slow; do
  flags=(); [ "$tier" = fast ] || flags=(--slow)
  "$RYII" cleave packages/signals/src/2_Signal.ts#isSignal packages/signals/src/2a_isSignal.ts --state "$STATE/$tier" "${flags[@]}" > ".dogfood/D05.$tier.plan"
done
"$RYII" cleave packages/signals/src/2_Signal.ts#isSignal packages/signals/src/2a_isSignal.ts --state "$STATE" --commit > .dogfood/D05.commit
rg 'export.*\{ isSignal \}.*2a_isSignal' packages/signals/src/2_Signal.ts
rg 'export.*isSignal' packages/signals/src/2a_isSignal.ts
echo 'D5: source module retains the public isSignal export'
