#!/usr/bin/env bash
# usage: 8_size.sh [small|crates ...] -> db/s1.tsv (corpus, format, bytes). Same data = the 7 scm++ tables (dicts, node, capture)
# of db/ancestor-<corpus>.db. Inputs: W1 outputs (db/w1-<corpus>.*), L1 outputs (db/ancestor-<corpus>.duckdb, db/parquet-<corpus>/).
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
sqlite=${SQLITE:-/opt/homebrew/opt/sqlite/bin/sqlite3}
out=$here/db/s1.tsv
printf 'corpus\tformat\tbytes\n' > "$out"
bytes() { /usr/bin/stat -f %z "$@" | awk '{s += $1} END {print s}'; }
for corpus in ${@:-small crates}; do
  tmp=$here/db/s1-$corpus.db
  emit() { printf '%s\t%s\t%s\n' "$corpus" "$1" "$2" >> "$out"; }
  emit ryii_store_as_written "$(bytes "$here/db/ancestor-$corpus.db")"
  emit sqlite_ryii_writer_with_indexes "$(bytes "$here/db/w1-$corpus.sqlite")"
  rm -f "$tmp"; cp "$here/db/w1-$corpus.sqlite" "$tmp"
  "$sqlite" "$tmp" "VACUUM;"
  emit sqlite_vacuum_with_indexes "$(bytes "$tmp")"
  "$sqlite" "$tmp" "DROP INDEX scmpp_node_sibling; DROP INDEX scmpp_capture_node; VACUUM;"
  emit sqlite_vacuum_tables_only "$(bytes "$tmp")"
  "$sqlite" "$tmp" "PRAGMA page_size = 4096; VACUUM;"
  emit sqlite_vacuum_tables_only_page4096 "$(bytes "$tmp")"
  emit duckdb_ctas_copy "$(bytes "$here/db/ancestor-$corpus.duckdb")"
  emit duckdb_appender "$(bytes "$here/db/w1-$corpus.duckdb")"
  emit duckdb_appender_primary_keys "$(bytes "$here/db/w1-$corpus.duckdb_pk")"
  emit parquet_snappy "$(bytes "$here/db/parquet-$corpus"/*.parquet)"
  rm -f "$tmp"
done
