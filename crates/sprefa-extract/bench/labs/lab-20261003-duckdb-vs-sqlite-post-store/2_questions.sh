#!/usr/bin/env bash
# usage: 2_questions.sh [small|crates ...] -> raw.tsv rows (run Q2), db/out/Q2/<corpus>/<question>.<engine>-query.rows
# SQLite runs on ryii's store as written (no index, no ANALYZE); DuckDB on native copies of node, resolved_edge, resolved_import.
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
sqlite=${SQLITE:-/opt/homebrew/opt/sqlite/bin/sqlite3}
duckdb=${DUCKDB:-/opt/homebrew/bin/duckdb}
section() { awk -v want="-- @$2" '/^-- @/{on = ($0 == want); next} on' "$1"; }
for corpus in ${@:-small crates}; do
  store=$here/db/resolve-$corpus.db duck=$here/db/resolve-$corpus.duckdb out=$here/db/out/Q2/$corpus
  mkdir -p "$out"
  if [[ ! -f $duck ]]; then
    REPEAT=1 python3 "$here/0_measure.py" Q2 load "$corpus" duckdb load "$out/load.txt" -- "$duckdb" "$duck" -c \
      "LOAD sqlite; ATTACH '$store' AS src (TYPE sqlite, READ_ONLY);
       CREATE TABLE node AS SELECT * FROM src.node; CREATE TABLE resolved_edge AS SELECT * FROM src.resolved_edge;
       CREATE TABLE resolved_import AS SELECT * FROM src.resolved_import;"
  fi
  for question in callers reaches importers cycles; do
    sql=$(section "$here/2_questions.sql" "$question")
    python3 "$here/0_measure.py" Q2 "$question" "$corpus" sqlite query "$out/$question.sqlite-query.rows" -- \
      "$sqlite" -batch -list -separator $'\x1f' -nullvalue NULL "$store" "$sql"
    python3 "$here/0_measure.py" Q2 "$question" "$corpus" duckdb query "$out/$question.duckdb-query.rows" -- \
      "$duckdb" -batch -noheader -list -separator $'\x1f' -nullvalue NULL "$duck" -c "$sql"
  done
done
