#!/bin/bash
set -euo pipefail
mkdir -p .dogfood/D10
cat > .dogfood/D10/0_globals.ts <<'TS'
export function useGlobals(){requestAnimationFrame(()=>{}); return new URL('https://example.org')}
export function local(){invalidate(); const invalidate=()=>0}
export function shadow(g:()=>void){g()}
TS
cat > .dogfood/D10/1_twins.ts <<'TS'
export const requestAnimationFrame=()=>0
export const URL=()=>0
export function invalidate(){return 1}
export function g(){return 1}
TS
"$RYII" --resolve --sqlite "$STATE/D10-repro.db" .dogfood/D10 >/dev/null
[ "$(sqlite3 "$STATE/D10-repro.db" "select count(*) from resolved_edge where caller_path like '%0_globals.ts' and callee_path like '%1_twins.ts';")" = 0 ]
[ "$(sqlite3 "$STATE/D10-repro.db" "select count(*) from resolved_edge where caller_name='local' and callee_name='invalidate' and resolution_origin='same_file';")" = 1 ]
"$RYII" --resolve --sqlite "$STATE/D10.db" packages/signal-grid packages/trace >/dev/null
[ "$(sqlite3 "$STATE/D10.db" "select count(*) from resolved_edge where callee_name='requestAnimationFrame';")" = 0 ]
"$RYII" --resolve --sqlite "$STATE/D10-all.db" packages >/dev/null
[ "$(sqlite3 "$STATE/D10-all.db" "select count(*) from resolved_edge where callee_name in ('URL','requestAnimationFrame') and resolution_origin='corpus_unique';")" = 0 ]
echo 'D10: DOM globals stay external; lexical invalidate binds locally; parameter shadow stays unbound'
