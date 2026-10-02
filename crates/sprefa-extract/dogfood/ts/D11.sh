#!/bin/bash
set -euo pipefail
"$RYII" --resolve --sqlite "$STATE/D11.db" packages/signals/src >/dev/null
[ "$(sqlite3 "$STATE/D11.db" "select count(*) from resolved_edge where caller_path like '%/1_SignalCreator.ts' and callee_path like '%/4_Query.ts' and callee_name='invalidate';")" = 0 ]
"$RYII" stratify packages/signals/src --from packages/signals/src/index.ts > "$STATE/D11.jsonl"
jq -se 'any(.[]; .record == "stratum_cycle" and (.paths | map(split("/")[-1]) | sort) == ["3_Endpoint.ts","4_Query.ts"]) and all(.[]; .record != "stratum_cycle" or all(.paths[]; endswith("/1_SignalCreator.ts")|not))' "$STATE/D11.jsonl" >/dev/null
echo 'D11: invalidate phantom edge absent; cycle is Endpoint/Query'
