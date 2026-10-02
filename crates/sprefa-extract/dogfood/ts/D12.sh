#!/bin/bash
set -euo pipefail
"$RYII" stratify packages/signals/src --from packages/signals/src/index.ts > "$STATE/D12.jsonl"
jq -se '
  [.[] | select(.record == "stratify_move")] as $moves |
  ($moves | length) > 0 and
  all($moves[];
    (.from_path | split("/")[-1] | sub("^[0-9]+[A-Za-z]*_"; "")) as $stem |
    (.to_path | split("/")[-1] | sub("^[0-9]+_"; "")) == $stem
  )' "$STATE/D12.jsonl" >/dev/null
mkdir -p .dogfood/D12
printf 'export function log(){return 1}\n' > .dogfood/D12/0_log.ts
printf 'import {log} from "./0_log.js"; export function slice(){return log()}\n' > .dogfood/D12/10_slice.ts
printf 'import {slice} from "./10_slice.js"; export function signal(){return slice()}\n' > .dogfood/D12/2a_Signal.ts
printf 'export * from "./2a_Signal.js"\n' > .dogfood/D12/index.ts
"$RYII" stratify .dogfood/D12 --from .dogfood/D12/index.ts > "$STATE/D12-repro.jsonl"
jq -se '
  [.[] | select(.record == "stratify_move")] as $moves |
  ($moves | length) == 3 and
  all($moves[];
    (.from_path | split("/")[-1] | sub("^[0-9]+[A-Za-z]*_"; "")) as $stem |
    (.to_path | split("/")[-1] | sub("^[0-9]+_"; "")) == $stem
  )' "$STATE/D12-repro.jsonl" >/dev/null
echo 'D12: stratify replaces numeric and insertion prefixes in corpus and repro'
