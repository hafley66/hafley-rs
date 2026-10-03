#!/usr/bin/env bash
# usage: 0_stores.sh [small|crates ...]   (FORCE=1 rebuilds) -> db/ancestor-<corpus>.db, db/resolve-<corpus>.db, db/lab-<corpus>.duckdb, db/sql/ancestor.sql
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd -- "$here/../../../../.." && pwd)
ryii=${RYII:-$repo/crates/sprefa-extract/target/release/ryii}
sql_bin=${SQL_BIN:-$here/4_writer/target/sql/release/lab_writer}
store_lab=$here/../lab-20261003-scmpp-sqlite-vs-duckdb
recursion_db=${RECURSION_DB:-/Users/chrishafley/projects/hafley-rs/.boop-worktrees/lab/recursion/crates/sprefa-extract/bench/labs/lab-20261003-recursion-pgq-gritql-sprefa/db}
db=$here/db
mkdir -p "$db/sql"
"$sql_bin" sql "$store_lab/ancestor.scm" > "$db/sql/ancestor.sql"
cd "$repo"
for corpus in ${@:-small crates}; do
  case $corpus in
    small) inputs=(crates/hafley_scm/src crates/sprefa-extract/src) ;;
    crates) inputs=(crates) ;;
  esac
  if [[ ${FORCE:-} == 1 || ! -f $db/ancestor-$corpus.db ]]; then
    rm -f "$db/ancestor-$corpus.db"
    REPEAT=1 python3 "$here/0_measure.py" store ancestor "$corpus" ryii scmpp "$db/out/store/ancestor-$corpus.txt" -- \
      "$ryii" query --scmpp "$store_lab/ancestor.scm" --sqlite "$db/ancestor-$corpus.db" --pattern '*.rs' "${inputs[@]}"
  fi
  if [[ ${FORCE:-} == 1 || ! -f $db/resolve-$corpus.db ]]; then
    rm -f "$db/resolve-$corpus.db"
    REPEAT=1 python3 "$here/0_measure.py" store resolve "$corpus" ryii resolve "$db/out/store/resolve-$corpus.txt" -- \
      "$ryii" --resolve --kinds cst,call --sqlite "$db/resolve-$corpus.db" --pattern '*.rs' "${inputs[@]}"
  fi
  # chain links (M1): the recursion lab's DuckDB store, copied; the source stays untouched
  [[ -f $db/lab-$corpus.duckdb ]] || cp "$recursion_db/lab-$corpus.duckdb" "$db/lab-$corpus.duckdb"
done
