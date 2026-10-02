#!/bin/bash
set -euo pipefail
before=$(git status --porcelain)
for action in plan commit; do
  flags=(); [ "$action" = plan ] || flags=(--commit)
  if "$RYII" rename packages/signals/src/2_Signal.ts#isSignal isSignalProbe --state "$PWD/.dogfood/0_insideState" "${flags[@]}" > ".dogfood/D22.$action" 2>&1; then
    echo 'D22: state inside target root unexpectedly accepted'; exit 1
  else rc=$?; fi
  printf '%s\n' "$rc" > ".dogfood/D22.$action.rc"
  rg 'state root must be outside target root' ".dogfood/D22.$action"
  [ ! -d .dogfood/0_insideState ]
  [ "$before" = "$(git status --porcelain)" ]
done
cmp .dogfood/D22.plan.rc .dogfood/D22.commit.rc
"$RYII" rename packages/signals/src/2_Signal.ts#isSignal isSignalProbe --state "$STATE" > .dogfood/D22.external-plan
"$RYII" rename packages/signals/src/2_Signal.ts#isSignal isSignalProbe --state "$STATE" --commit > .dogfood/D22.external-commit
echo 'D22: both modes refuse internal state before creating it and accept external state'
