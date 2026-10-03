#!/usr/bin/env bash
# usage: 3_duckpgq.sh [small|crates ...]  -> arm P1 (DuckDB CLI + duckpgq, threads 1 and default); graphs live in db/pgq-<corpus>.duckdb
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "$here/0_measure.sh"
duckdb=${DUCKDB:-$here/db/bin/duckdb}
sql=$here/3_duckpgq.sql
for corpus in ${@:-small crates}; do
  store=$here/db/pgq-$corpus.duckdb
  rm -f "$store" "$store.wal"
  cp "$here/db/lab-$corpus.duckdb" "$store"
  start=$(date +%s.%N)
  "$duckdb" -cmd "INSTALL duckpgq FROM community; LOAD duckpgq;" "$store" -c "$(section "$sql" setup)" > /dev/null
  setup=$(python3 -c "import sys; print(f'{float(sys.argv[2]) - float(sys.argv[1]):.2f}')" "$start" "$(date +%s.%N)")
  for job in ${JOBS:-T1:cst T3:fast T3:slow T4:fast T4:slow}; do
    task=${job%%:*} tier=${job##*:}
    query=$(section "$sql" "$task" | sed "s/{tier}/$tier/g")
    lines=$(printf '%s\n' "$query" | wc -l | tr -d ' ')
    for threads in ${THREADS:-1 default}; do
      set_threads=$([[ $threads == 1 ]] && echo "SET threads = 1;" || echo "RESET threads;")
      measure P1 "$task" "$corpus" "$tier" "$threads" "$setup" "$lines" "P1_${task}_${corpus}_${tier}_t$threads" -- \
        "$duckdb" -header -list -separator $'\t' -cmd "LOAD duckpgq; $set_threads" "$store" -c "$query"
    done
  done
done
