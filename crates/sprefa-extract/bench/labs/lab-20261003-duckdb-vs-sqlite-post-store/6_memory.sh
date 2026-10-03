#!/usr/bin/env bash
# usage: 6_memory.sh [crates|small] -> raw.tsv rows (run M1, variant mem<MB>_t<threads>); stdout file holds temporary_storage_bytes
# DuckDB only: has-ancestor (scm++ store, native copy from 3_load_paths.sh) and chain links (recursion lab T1, lab-<corpus>.duckdb copy).
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
duckdb=${DUCKDB:-/opt/homebrew/bin/duckdb}
section() { awk -v want="-- @$2" '/^-- @/{on = ($0 == want); next} on' "$1"; }
corpus=${1:-crates}
ancestor=$(cat "$here/db/sql/ancestor.sql")
chain=$(section "$here/../lab-20261003-recursion-pgq-gritql-sprefa/5_recursive_cte.sql" T1 | sed 's/;[[:space:]]*$//')
out=$here/db/out/M1/$corpus
mkdir -p "$out" "$here/db/spill"
for memory in 64 128 256 default; do
  for threads in 1 4 default; do
    set=""
    [[ $memory == default ]] || set+="SET memory_limit = '${memory}MB'; "
    [[ $threads == default ]] || set+="SET threads = $threads; "
    set+="SET temp_directory = '$here/db/spill'; "
    for query in ancestor chain; do
      case $query in ancestor) file=$here/db/ancestor-$corpus.duckdb sql=$ancestor ;; chain) file=$here/db/lab-$corpus.duckdb sql=$chain ;; esac
      python3 "$here/0_measure.py" M1 "$query" "$corpus" duckdb "mem${memory}_t$threads" "$out/$query.mem${memory}_t$threads.txt" -- \
        "$duckdb" -readonly -batch -noheader -list "$file" -c \
        "$set CREATE TEMP TABLE bench_row AS $sql; SELECT count(*) FROM bench_row; SELECT sum(temporary_storage_bytes) FROM duckdb_memory();"
    done
  done
done
