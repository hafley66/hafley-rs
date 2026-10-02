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
rg -q 'error:' "$scratch/err"
# Root PATH positionals can make clap report the unsupported --state option
# instead of naming an unknown subcommand. Neither form may reach a TODO.
if rg -q 'TODO|not implemented' "$scratch/err"; then exit 1; fi
[ ! -s "$scratch/out" ]
echo 'D23 unimplemented dismantle removed from the command surface; invocation rejected by argument parsing'
