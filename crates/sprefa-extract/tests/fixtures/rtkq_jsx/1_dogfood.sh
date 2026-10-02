#!/usr/bin/env bash
# Fact-row assertions only. Supply an already built ryii by absolute path.
set -euo pipefail
: "${RYII:?Set RYII to an already built ryii binary (absolute path)}"
[[ "$RYII" == /* ]] || { printf '%s\n' 'RYII must be an absolute path' >&2; exit 2; }
case_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$case_dir/../../.."
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
export RUST_LOG=off DL_TRAIL=0
files=(tests/fixtures/rtkq_jsx/{components.tsx,0_nested.tsx,hooks.ts})
normalize='select(.record=="call_site" or .record=="jsx_element" or .record=="jsx_attribute") | .path |= split("/")[-1]'
jq -Sc "$normalize" tests/goldens/193_ts_syntax.jsonl | LC_ALL=C sort > "$scratch/expected.jsonl"

for mode in fast --resolve; do
  "$RYII" "$mode" "${files[@]}" > "$scratch/$mode.jsonl"
  jq -Sc "$normalize" "$scratch/$mode.jsonl" | LC_ALL=C sort > "$scratch/actual.jsonl"
  diff -u "$scratch/expected.jsonl" "$scratch/actual.jsonl"
  "$RYII" "$mode" "${files[@]}" --sqlite "$scratch/$mode.db"
  # Read the stored payload columns verbatim, without derived graph analysis.
  sqlite3 "$scratch/$mode.db" <<'SQL' > "$scratch/stored.jsonl"
SELECT json_object('record',record,'callee',callee,'path',path,'line',line,'fn',"fn",'start',start,'end',end) FROM call_site
UNION ALL
SELECT json_object('record',record,'name',name,'path',path,'line',line,'fn',"fn",'start',start,'end',end,'parent_start',parent_start) FROM jsx_element
UNION ALL
SELECT json_object('record',record,'path',path,'element_start',element_start,'name',name,'value',value,'start',start,'end',end) FROM jsx_attribute;
SQL
  jq -Sc "$normalize" "$scratch/stored.jsonl" | LC_ALL=C sort > "$scratch/actual.jsonl"
  diff -u "$scratch/expected.jsonl" "$scratch/actual.jsonl"
done
