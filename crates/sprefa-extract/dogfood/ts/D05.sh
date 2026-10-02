#!/bin/bash
set -euo pipefail
CASE=D05
source "$(dirname "${BASH_SOURCE[0]}")/1_type_errors.sh"
type_baseline signals signal-grid docs-kit
for tier in fast slow; do
  flags=(); [ "$tier" = fast ] || flags=(--slow)
  "$RYII" cleave packages/signals/src/2_Signal.ts#isSignal packages/signals/src/2a_isSignal.ts --state "$STATE/$tier" "${flags[@]}" > ".dogfood/D05.$tier.plan"
done
"$RYII" cleave packages/signals/src/2_Signal.ts#isSignal packages/signals/src/2a_isSignal.ts --state "$STATE" --commit > .dogfood/D05.commit
rg 'export.*\{ isSignal \}.*2a_isSignal' packages/signals/src/2_Signal.ts
rg 'export.*isSignal' packages/signals/src/2a_isSignal.ts
type_no_new signals signal-grid docs-kit
echo 'D5: source module retains the public isSignal export'
