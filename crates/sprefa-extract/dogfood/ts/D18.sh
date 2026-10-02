#!/bin/bash
set -euo pipefail
export RUST_LOG=warn DL_TRAIL=0
scratch=$(mktemp -d "$STATE/D18.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
query='(call_expression) @c'
"$RYII" query --query "$query" packages/md > "$scratch/mixed.jsonl" 2> "$scratch/mixed.err"
"$RYII" query --query "$query" --pattern '**/*.ts' --pattern '**/*.tsx' packages/md > "$scratch/ts.jsonl" 2> "$scratch/ts.err"
jq -Sc . "$scratch/mixed.jsonl" | sort > "$scratch/mixed.sorted"
jq -Sc . "$scratch/ts.jsonl" | sort > "$scratch/ts.sorted"
cmp "$scratch/mixed.sorted" "$scratch/ts.sorted"
[ -s "$scratch/mixed.sorted" ]
rg -q 'query: skipped [1-9][0-9]* files whose grammar lacks a query node type' "$scratch/mixed.err"
# Malformed syntax must remain an error, rather than a grammar skip.
if "$RYII" query --query '(call_expression' packages/md/src/lib/1_tableModel.ts > "$scratch/bad.out" 2> "$scratch/bad.err"; then
  exit 1
fi
echo "D18 mixed grammar matches equal TS/TSX matches; $(rg 'query: skipped' "$scratch/mixed.err")"
