#!/bin/bash
set -euo pipefail
"$RYII" --resolve --sqlite "$STATE/D13.db" packages/md >/dev/null
sites=$(sqlite3 "$STATE/D13.db" "select count(*) from (select distinct caller_path,caller_site_start,caller_site_end,callee_path,callee_start,callee_end from resolved_edge where callee_name='markdownTableModel');")
[ "$sites" = 6 ]
for tier in fast slow; do
  flags=(); [ "$tier" = fast ] || flags=(--slow)
  "$RYII" graph --callers markdownTableModel "${flags[@]}" packages/md > "$STATE/D13-$tier.jsonl"
  [ "$(jq -s 'length' "$STATE/D13-$tier.jsonl")" = "$sites" ]
done
echo 'D13: existing callers command returns six sites in both tiers; mirror facts remain'
