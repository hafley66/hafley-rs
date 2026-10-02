#!/bin/bash
set -euo pipefail
mkdir -p .dogfood/D09
cat > .dogfood/D09/0_calls.ts <<'TS'
function f(){return 1}
const g=()=>2
const k=function(){return 3}
export function h(){return f()+g()+k()}
function nested(){const inner=()=>4; return inner()}
TS
"$RYII" --resolve --sqlite "$STATE/D09.db" .dogfood/D09 >/dev/null
for name in g k inner; do
  [ "$(sqlite3 "$STATE/D09.db" "select count(*) from resolved_edge where callee_name='$name' and resolution_origin='same_file';")" = 1 ]
done
[ "$("$RYII" graph --callers sectionsOf packages/md 2>/dev/null | jq -s 'length')" -gt 0 ]
echo 'D09: g, k, inner each resolve once; corpus sectionsOf has callers'
