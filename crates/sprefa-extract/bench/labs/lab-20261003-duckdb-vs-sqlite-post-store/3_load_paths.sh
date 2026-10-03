#!/usr/bin/env bash
# usage: 3_load_paths.sh [small|crates ...] -> raw.tsv rows (run L1); rows files db/out/L1/<corpus>/ancestor.<engine>-<variant>.rows
# ryii's scm++ store into DuckDB: (a) attach = sqlite_scanner in place, (b) copy = native tables, (c) parquet = COPY TO
# parquet then read_parquet views. Load and has-ancestor query timed apart; the query is CREATE TEMP TABLE AS <sql>.
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
sqlite=${SQLITE:-/opt/homebrew/opt/sqlite/bin/sqlite3}
duckdb=${DUCKDB:-/opt/homebrew/bin/duckdb}
tables="scmpp_dict_path scmpp_dict_kind scmpp_dict_field scmpp_dict_capture scmpp_dict_text scmpp_node scmpp_capture"
sql=$(cat "$here/db/sql/ancestor.sql")
dump=(-batch -noheader -list -separator $'\x1f' -newline $'\x1e' -nullvalue NULL)
m() { python3 "$here/0_measure.py" L1 ancestor "$@"; }
for corpus in ${@:-small crates}; do
  store=$here/db/ancestor-$corpus.db out=$here/db/out/L1/$corpus pq=$here/db/parquet-$corpus duck=$here/db/ancestor-$corpus.duckdb
  mkdir -p "$out"
  attach="LOAD sqlite; ATTACH '$store' AS s (TYPE sqlite, READ_ONLY); USE s;"
  copy="LOAD sqlite; ATTACH '$store' AS s (TYPE sqlite, READ_ONLY);"
  export="LOAD sqlite; ATTACH '$store' AS s (TYPE sqlite, READ_ONLY);"
  views=
  for table in $tables; do
    copy+=" CREATE TABLE main.$table AS SELECT * FROM s.$table;"
    export+=" COPY (SELECT * FROM s.$table) TO '$pq/$table.parquet' (FORMAT parquet);"
    views+=" CREATE VIEW $table AS SELECT * FROM read_parquet('$pq/$table.parquet');"
  done
  # SQLite reference: the store as ryii left it (its two post-load indexes, no ANALYZE)
  m "$corpus" sqlite query "$out/sqlite.time" -- "$sqlite" -readonly "$store" "PRAGMA temp_store=MEMORY; CREATE TEMP TABLE bench_row AS $sql;"
  "$sqlite" -batch -list -separator $'\x1f' -newline $'\x1e' -nullvalue NULL -readonly "$store" "$sql;" > "$out/ancestor.sqlite-query.rows"
  # (a) attach
  # count(*) over an attached WITHOUT ROWID table fails in sqlite_scanner (it selects ROWID); recorded as its own variant
  REPEAT=1 m "$corpus" duckdb attach_count_star "$out/attach.count" -- "$duckdb" :memory: -c "$attach SELECT count(*) FROM scmpp_node;" || true
  m "$corpus" duckdb attach_load "$out/attach.load" -- "$duckdb" :memory: -c "$attach SELECT count(file) FROM scmpp_node;"
  m "$corpus" duckdb attach_query "$out/attach.time" -- "$duckdb" :memory: -c "$attach CREATE TEMP TABLE bench_row AS $sql;"
  "$duckdb" "${dump[@]}" :memory: -c "$attach $sql;" > "$out/ancestor.duckdb-attach_query.rows"
  # (b) copy
  rm -f "$duck"
  REPEAT=1 m "$corpus" duckdb copy_load "$out/copy.load" -- "$duckdb" "$duck" -c "$copy"
  m "$corpus" duckdb copy_query "$out/copy.time" -- "$duckdb" -readonly "$duck" -c "CREATE TEMP TABLE bench_row AS $sql;"
  "$duckdb" "${dump[@]}" -readonly "$duck" -c "$sql;" > "$out/ancestor.duckdb-copy_query.rows"
  # (c) parquet
  rm -rf "$pq"; mkdir -p "$pq"
  REPEAT=1 m "$corpus" duckdb parquet_load "$out/parquet.load" -- "$duckdb" :memory: -c "$export"
  m "$corpus" duckdb parquet_query "$out/parquet.time" -- "$duckdb" :memory: -c "$views CREATE TEMP TABLE bench_row AS $sql;"
  "$duckdb" "${dump[@]}" :memory: -c "$views $sql;" > "$out/ancestor.duckdb-parquet_query.rows"
done
