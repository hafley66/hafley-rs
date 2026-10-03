#!/usr/bin/env bash
# usage: 4_postgres_pgq.sh  -> arm P2: records not_available unless a bottled Homebrew PostgreSQL with GRAPH_TABLE exists
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "$here/0_measure.sh"
checked=$(for formula in postgresql@17 postgresql@18 postgresql@19; do
  brew info --json=v2 "$formula" 2>/dev/null | python3 -c "
import json, sys
f = json.load(sys.stdin)['formulae'][0]
print(f\"{f['name']} {f['versions']['stable']} bottle={'yes' if f['versions']['bottle'] else 'no'}\", end='; ')" 2>/dev/null || printf '%s absent; ' "$formula"
done)
note="checked: ${checked}GRAPH_TABLE first ships after PostgreSQL 18; no bottled formula carries it; source builds excluded"
for job in T1:cst T3:fast T3:slow T4:fast T4:slow; do
  printf 'P2\t%s\t-\t%s\t-\t\t\t\t%s\tnot_available\t%s\tP2_%s_%s\n' "${job%%:*}" "${job##*:}" \
    "$(wc -l < "$here/4_postgres_pgq.sql" | tr -d ' ')" "$note" "${job%%:*}" "${job##*:}" >> "$here/db/runs.tsv"
done
echo "$note"
