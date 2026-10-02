#!/bin/bash
set -euo pipefail
export RUST_LOG=warn DL_TRAIL=0
scratch=$(mktemp -d "$STATE/D21.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
"$RYII" --resolve --sqlite "$scratch/facts.db" --pattern '**/*.ts' packages
"$RYII" fast --pattern '**/*.ts' packages > "$scratch/fast.jsonl"
jq -c 'select(.record == "occurrence") | [.symbol,.path,.start,.end,.role,(if .exported then 1 else 0 end),.decl_start,.decl_end]' "$scratch/fast.jsonl" | sort > "$scratch/fast.occurrences"
sqlite3 "$scratch/facts.db" 'SELECT json_array(symbol,path,start,end,role,exported,decl_start,decl_end) FROM occurrence;' | sort > "$scratch/sqlite.occurrences"
[ -s "$scratch/fast.occurrences" ]
cmp "$scratch/fast.occurrences" "$scratch/sqlite.occurrences"
echo "D21 $(wc -l < "$scratch/sqlite.occurrences" | tr -d ' ') occurrence rows equal fast JSONL, including duplicates and every field"
