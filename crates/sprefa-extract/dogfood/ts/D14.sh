#!/bin/bash
set -euo pipefail
: "${RYII:?}" "${STATE:?}"
RUST_LOG=off "$RYII" graph --callers render --slow --sqlite "$STATE/D14.graph.db" packages/md \
  >"$STATE/D14.graph.jsonl"
jq -se '[.[] | select(.record == "graph_edge"
    and (.from_path | endswith("src/lib/0_diagramRenderCache.test.ts"))
    and (.to_path | endswith("src/lib/0_diagramRenderCache.ts"))) | .from_line]
    | unique == [7,8,10,11,12,14,15,28,29,30,31,33]' "$STATE/D14.graph.jsonl" >/dev/null
RUST_LOG=off "$RYII" --resolve --ts-checker --root . --sqlite "$STATE/D14.resolve.db" packages/md >/dev/null
RUST_LOG=off "$RYII" slow --root . --sqlite "$STATE/D14.slow.db" packages/md >/dev/null
for tier in graph resolve slow; do
  count=$(sqlite3 "$STATE/D14.$tier.db" "SELECT count(DISTINCT caller_site_start) FROM resolved_edge
    WHERE caller_path LIKE '%/src/lib/0_diagramRenderCache.test.ts'
      AND callee_path LIKE '%/src/lib/0_diagramRenderCache.ts'
      AND callee_name = 'render' AND resolution_origin = 'checker';")
  [[ "$count" == 12 ]] || { echo "D14 $tier: $count of 12 checker-bound cache.render sites"; exit 1; }
done
echo 'D14 graph, --resolve, slow: all 12 cache.render sites bind to the class method in facts'
