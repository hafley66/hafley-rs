#!/bin/bash
set -euo pipefail
: "${RYII:?}" "${STATE:?}"
anchor=packages/md/src/lib/0_diagramRenderCache.ts
if RUST_LOG=off "$RYII" rename "$anchor#render" renderCached --state "$STATE" \
    >"$STATE/D17.out" 2>"$STATE/D17.err"; then
  echo 'D17 expected a refusal for unresolved method receivers'; exit 1
fi
[[ ! -s "$STATE/D17.out" ]]
[[ $(wc -l <"$STATE/D17.err") -eq 1 ]]
rg -q -- '--slow' "$STATE/D17.err"
[[ -z $(git diff -- "$anchor") ]]
echo 'D17 fast method rename: one-line non-zero refusal naming --slow; source unchanged'
