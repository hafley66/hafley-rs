#!/usr/bin/env bash
# usage: 5_recursive_cte.sh [small|crates ...]  -> arms R1 (sqlite3 CLI) and R2 (DuckDB CLI, threads 1 and default)
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "$here/0_measure.sh"
sqlite=${SQLITE:-/opt/homebrew/opt/sqlite/bin/sqlite3}
duckdb=${DUCKDB:-$here/db/bin/duckdb}
sql=$here/5_recursive_cte.sql
for corpus in ${@:-small crates}; do
  for job in T1:cst T3:fast T3:slow T4:fast T4:slow; do
    task=${job%%:*} tier=${job##*:}
    query=$(section "$sql" "$task" | sed "s/{tier}/$tier/g")
    lines=$(printf '%s\n' "$query" | wc -l | tr -d ' ')
    measure R1 "$task" "$corpus" "$tier" 1 0 "$lines" "R1_${task}_${corpus}_${tier}" -- \
      "$sqlite" -header -separator $'\t' "$here/db/lab-$corpus.db" "$query"
    for threads in 1 default; do
      set_threads=$([[ $threads == 1 ]] && echo "SET threads = 1;" || echo "RESET threads;")
      measure R2 "$task" "$corpus" "$tier" "$threads" 0 "$lines" "R2_${task}_${corpus}_${tier}_t$threads" -- \
        "$duckdb" -readonly -header -list -separator $'\t' -cmd "$set_threads" "$here/db/lab-$corpus.duckdb" -c "$query"
      if [[ $task != T1 ]]; then
        keyed=$(section "$sql" "${task}_key" | sed "s/{tier}/$tier/g")
        measure R2_key "$task" "$corpus" "$tier" "$threads" 0 "$(printf '%s\n' "$keyed" | wc -l | tr -d ' ')" \
          "R2_key_${task}_${corpus}_${tier}_t$threads" -- \
          "$duckdb" -readonly -header -list -separator $'\t' -cmd "$set_threads" "$here/db/lab-$corpus.duckdb" -c "$keyed"
      fi
    done
  done
done
