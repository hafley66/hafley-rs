#!/bin/bash
set -euo pipefail
export RUST_LOG=warn DL_TRAIL=0
scratch=$(mktemp -d "$STATE/D23.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
"$RYII" --help > "$scratch/help"
if rg -q 'dismantle|TODO' "$scratch/help"; then exit 1; fi
rc=0
"$RYII" dismantle packages/md/src/lib/1_tableModel.ts#CodeToken --state "$STATE" > "$scratch/out" 2> "$scratch/err" || rc=$?
[ "$rc" -eq 2 ]
rg -q "unrecognized subcommand 'dismantle'" "$scratch/err"
[ ! -s "$scratch/out" ]
echo 'D23 unimplemented dismantle removed from the command surface; invocation rejected by argument parsing'
