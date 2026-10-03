#!/usr/bin/env bash
# usage: 5_cold_warm.sh [small|crates ...] -> db/c1.tsv (engine, corpus, process, iteration, wall_s, load_average_1m)
# One process per repeat opens the store fresh and runs has-ancestor ITERATIONS times (CREATE TEMP TABLE bench_row_<i> AS <sql>);
# per-statement time from each CLI's .timer. Iteration 1 = first query after process start. OS page cache stays warm (no purge).
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
sqlite=${SQLITE:-/opt/homebrew/opt/sqlite/bin/sqlite3}
duckdb=${DUCKDB:-/opt/homebrew/bin/duckdb}
iterations=${ITERATIONS:-5}
sql=$(cat "$here/db/sql/ancestor.sql")
out=$here/db/c1.tsv
[[ -f $out ]] || printf 'engine\tcorpus\tprocess\titeration\twall_s\tload_average_1m\n' > "$out"
script=".timer on"$'\n'
for i in $(seq 1 "$iterations"); do script+="CREATE TEMP TABLE bench_row_$i AS $sql;"$'\n'; done
for corpus in ${@:-small crates}; do
  for process in 1 2 3; do
    load=$(sysctl -n vm.loadavg | awk '{print $2}')
    printf '%s' "PRAGMA temp_store=MEMORY;"$'\n'"$script" | "$sqlite" -readonly "$here/db/ancestor-$corpus.db" \
      | awk -v c="$corpus" -v p="$process" -v l="$load" '/^Run Time: real/{i++; printf "sqlite\t%s\t%s\t%d\t%s\t%s\n", c, p, i, $4, l}' >> "$out"
    printf '%s' "$script" | "$duckdb" -readonly "$here/db/ancestor-$corpus.duckdb" \
      | awk -v c="$corpus" -v p="$process" -v l="$load" '/^Run Time \(s\): real/{i++; printf "duckdb\t%s\t%s\t%d\t%s\t%s\n", c, p, i, $5, l}' >> "$out"
  done
done
