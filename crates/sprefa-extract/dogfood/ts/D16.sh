#!/bin/bash
set -euo pipefail
file=packages/md/src/lib/0_diagramRenderCache.ts
"$RYII" --resolve --sqlite "$STATE/D16.db" packages/md >/dev/null
sites=$(sqlite3 "$STATE/D16.db" "select count(*) from (select distinct caller_path,caller_site_start,caller_site_end,callee_path,callee_start,callee_end from resolved_edge where callee_name='render' and callee_path='$file');")
[ "$sites" -gt 0 ]
"$RYII" graph --callers "$file#render" packages/md > "$STATE/D16.jsonl"
jq -se --arg path "$file" --argjson count "$sites" 'length == $count and all(.[]; .to_path == $path and .to_name == "render")' "$STATE/D16.jsonl" >/dev/null
if "$RYII" graph --callers DiagramRenderCache.render packages/md > "$STATE/D16-qualified.jsonl" 2> "$STATE/D16-qualified.err"; then
  echo 'D16: Class.method silently accepted'; exit 1
fi
[ ! -s "$STATE/D16-qualified.jsonl" ]
rg -q 'Class.method is unsupported; use FILE#method' "$STATE/D16-qualified.err"
echo 'D16: FILE#render selects one file; Class.method produces an explicit usage error'
