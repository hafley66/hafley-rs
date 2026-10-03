#!/usr/bin/env bash
# usage: 2_engines.sh > engines.tsv
# The compiled scm++ SQL (ancestor.sql, plain.sql) over the store `ryii query --scmpp --sqlite` wrote
# (db/after-<query>-<corpus>.db from 1_before_after.sh), on the SQLite CLI and on the DuckDB CLI.
# DuckDB copies the scm++ tables into its own file first (sqlite extension); only the query is timed.
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
sqlite=${SQLITE:-/opt/homebrew/opt/sqlite/bin/sqlite3}
duckdb=${DUCKDB:-duckdb}
tables="scmpp_dict_path scmpp_dict_kind scmpp_dict_field scmpp_dict_capture scmpp_dict_text scmpp_node scmpp_capture"
measure() { # engine query corpus rows-check command...
  local engine=$1 query=$2 corpus=$3; shift 3
  /usr/bin/time -l "$@" >/dev/null 2>"$here/db/time.txt"
  wall=$(awk '/ real /{print $1}' "$here/db/time.txt")
  rss=$(awk '/maximum resident set size/{printf "%.0f", $1/1048576}' "$here/db/time.txt")
  printf '%s\t%s\t%s\t%s\t%s\n' "$engine" "$query" "$corpus" "$wall" "$rss"
}
printf 'engine\tquery\tcorpus\twall_s\tpeak_rss_mb\n'
for query in ancestor plain; do
  sql=$(cat "$here/$query.sql")
  for corpus in crates scm_extract_src; do
    store="$here/db/after-$query-$corpus.db"
    copy="$here/db/bench-$query-$corpus.db"
    cp -f "$store" "$copy"
    "$sqlite" "$copy" "DROP TABLE IF EXISTS scmpp_row"
    measure sqlite "$query" "$corpus" "$sqlite" "$copy" "CREATE TABLE bench_row AS $sql"
    duck="$here/db/bench-$query-$corpus.duckdb"
    rm -f "$duck"
    load="LOAD sqlite;"
    for table in $tables; do
      load+=" CREATE TABLE $table AS SELECT * FROM sqlite_scan('$store', '$table');"
    done
    "$duckdb" "$duck" -c "$load" >/dev/null
    measure duckdb "$query" "$corpus" "$duckdb" "$duck" -c "CREATE TABLE bench_row AS $sql"
    # Same rows in the same order; list mode with a unit separator, so neither CLI quotes.
    cmp -s <("$sqlite" -list -separator $'\x1f' "$copy" "SELECT * FROM bench_row ORDER BY rowid") \
           <("$duckdb" "$duck" -noheader -list -separator $'\x1f' -c "SELECT * FROM bench_row") \
      || echo "rows differ: $query $corpus" >&2
    rm -f "$copy" "$duck"
  done
done
