#!/bin/bash
set -euo pipefail
export RUST_LOG=warn DL_TRAIL=0
scratch=$(mktemp -d "$STATE/D25.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
path=packages/md/src/lib/1_tableModel.ts
"$RYII" graph --help > "$scratch/help"
rg -q 'PATH@START:END' "$scratch/help"
rg -q 'END exclusive' "$scratch/help"
"$RYII" --resolve --arms call,flow --sqlite "$scratch/facts.db" --pattern '**/*.ts' --pattern '**/*.tsx' packages/md
blob=$(sqlite3 "$scratch/facts.db" "SELECT digest FROM file WHERE path='$path';")
[ -n "$blob" ]
# Facts for the plan's children parameter and its direct local consumers.
[ "$(sqlite3 "$scratch/facts.db" "SELECT count(*) FROM node WHERE _input_path='$path' AND family='df' AND kind='param' AND name='children' AND span__start=3621 AND span__end=3640;")" -eq 1 ]
[ "$(sqlite3 "$scratch/facts.db" "SELECT count(*) FROM edge WHERE _input_path='$path' AND family='df' AND from__start=3621 AND from__end=3640;")" -gt 0 ]
"$RYII" graph --flow-path "$path@3621:3640" packages/md > "$scratch/path.jsonl" 2> "$scratch/path.err"
"$RYII" graph --flow-path "$blob@3621:3640" packages/md > "$scratch/blob.jsonl" 2> "$scratch/blob.err"
cmp "$scratch/path.jsonl" "$scratch/blob.jsonl"
# Keep the original expected result visible: input normalization alone does
# not add local df traversal to the existing interprocedural graph command.
[ -s "$scratch/path.jsonl" ]
echo "D25 PATH and tagged digest agree; paths from children exist"
