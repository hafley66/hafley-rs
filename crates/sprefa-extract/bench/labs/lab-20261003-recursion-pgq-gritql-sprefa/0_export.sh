#!/usr/bin/env bash
# usage: 0_export.sh [small|crates ...]   (FORCE=1 rebuilds the ryii stores); table list and identity rules: README.md
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd -- "$here/../../../../.." && pwd)
ryii=${RYII:-$repo/crates/sprefa-extract/target/release/ryii}
duckdb=${DUCKDB:-$here/db/bin/duckdb}
sqlite=${SQLITE:-/opt/homebrew/opt/sqlite/bin/sqlite3}
db=$here/db
mkdir -p "$db"
SEEDS=(
  crates/hafley_scm/src/atoms.rs#new
  crates/hafley_scm/src/lang/rust/11_df_syntax_rows.rs#push
  crates/hafley_scm/src/lang/rust/17_tree_entity_rows.rs#node_text
  crates/hafley_scm/src/atoms.rs#lookup
  crates/hafley_scm/src/lang/rust/11_df_syntax_rows.rs#df_edge
  crates/hafley_scm/src/lang/rust/11_df_syntax_rows.rs#span
  crates/hafley_scm/src/span.rs#node_span
  crates/hafley_scm/src/read/0_request_root.rs#io_path
  crates/hafley_scm/src/lang/rust/15_syntax.rs#parse_rust_file
  crates/hafley_scm/src/read/types.rs#corpus_defs
  crates/hafley_scm/src/lib.rs#build
  crates/sprefa-extract/src/0_graph.rs#run
)
timed() { # label command... ; appends label, wall_s, peak_rss_mb to db/export_time.tsv
  local label=$1; shift
  /usr/bin/time -l "$@" >/dev/null 2>"$db/time.txt" || { tail -5 "$db/time.txt" >&2; return 1; }
  printf '%s\t%s\t%s\n' "$label" "$(awk '/ real /{print $1}' "$db/time.txt")" \
    "$(awk '/maximum resident set size/{printf "%.0f", $1/1048576}' "$db/time.txt")" >> "$db/export_time.tsv"
}
printf '%s\n' '((identifier) @id (#has-ancestor? @id source_file))' > "$db/cst.scm"
corpora=("${@:-small crates}")
for corpus in ${corpora[@]}; do
  case $corpus in
    small) inputs=(crates/hafley_scm/src crates/sprefa-extract/src) ;;
    crates) inputs=(crates) ;;
  esac
  cd "$repo"
  if [[ ${FORCE:-} == 1 || ! -f $db/cst-$corpus.db ]]; then
    rm -f "$db/cst-$corpus.db"
    timed "cst_$corpus" "$ryii" query --scmpp "$db/cst.scm" --sqlite "$db/cst-$corpus.db" --pattern '*.rs' "${inputs[@]}"
  fi
  if [[ ${FORCE:-} == 1 || ! -f $db/calls-fast-$corpus.db ]]; then
    rm -f "$db/calls-fast-$corpus.db"
    timed "calls_fast_$corpus" "$ryii" --resolve --kinds cst,call --sqlite "$db/calls-fast-$corpus.db" --pattern '*.rs' "${inputs[@]}"
  fi
  if [[ ${FORCE:-} == 1 || ! -f $db/calls-slow-$corpus.db ]]; then
    rm -f "$db/calls-slow-$corpus.db"
    timed "calls_slow_$corpus" "$ryii" graph --slow --from crates/hafley_scm/src/lib.rs#build --timeout 3000 --root . \
      --sqlite "$db/calls-slow-$corpus.db" --pattern '*.rs' "${inputs[@]}"
  fi
  lab=$db/lab-$corpus.db
  rm -rf "$lab" "$db/lab-$corpus.duckdb" "$db/parquet-$corpus"
  cp "$db/cst-$corpus.db" "$lab"
  # keep the scm++ tables only
  "$sqlite" "$lab" "SELECT 'DROP ' || type || ' ' || name || ';' FROM sqlite_master
    WHERE type IN ('table', 'view') AND name NOT LIKE 'scmpp_%' AND name NOT LIKE 'sqlite_%'" | "$sqlite" "$lab"
  seed_rows=$(for anchor in "${SEEDS[@]}"; do printf "('%s', '%s', '%s')," "$anchor" "${anchor%%#*}" "${anchor##*#}"; done)
  "$sqlite" "$lab" <<SQL
ATTACH '$db/calls-fast-$corpus.db' AS fast;
ATTACH '$db/calls-slow-$corpus.db' AS slow;
CREATE TEMP TABLE edge_text AS
  SELECT DISTINCT 'fast' AS tier, caller_path, caller_name, callee_path, callee_name FROM fast.resolved_edge
  WHERE caller_name IS NOT NULL AND callee_name IS NOT NULL
  UNION
  SELECT DISTINCT 'slow', caller_path, caller_name, callee_path, callee_name FROM slow.resolved_edge
  WHERE caller_name IS NOT NULL AND callee_name IS NOT NULL;
CREATE TEMP TABLE seed_text(anchor TEXT, path TEXT, name TEXT);
INSERT INTO seed_text VALUES ${seed_rows%,};
CREATE TABLE fn_dict_path(id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
INSERT INTO fn_dict_path(text) SELECT p FROM (SELECT caller_path AS p FROM edge_text UNION SELECT callee_path FROM edge_text
  UNION SELECT path FROM seed_text) ORDER BY p;
CREATE TABLE fn_dict_name(id INTEGER PRIMARY KEY, text TEXT NOT NULL UNIQUE);
INSERT INTO fn_dict_name(text) SELECT n FROM (SELECT caller_name AS n FROM edge_text UNION SELECT callee_name FROM edge_text
  UNION SELECT name FROM seed_text) ORDER BY n;
CREATE TABLE fn(id INTEGER PRIMARY KEY, path_id INTEGER NOT NULL, name_id INTEGER NOT NULL, UNIQUE (path_id, name_id));
INSERT INTO fn(path_id, name_id)
  SELECT p.id, n.id FROM (SELECT caller_path AS path, caller_name AS name FROM edge_text
    UNION SELECT callee_path, callee_name FROM edge_text UNION SELECT path, name FROM seed_text) AS f
  JOIN fn_dict_path AS p ON p.text = f.path JOIN fn_dict_name AS n ON n.text = f.name ORDER BY p.id, n.id;
CREATE TEMP VIEW fn_text AS SELECT f.id, p.text AS path, n.text AS name FROM fn AS f
  JOIN fn_dict_path AS p ON p.id = f.path_id JOIN fn_dict_name AS n ON n.id = f.name_id;
CREATE TABLE call_edge_fast(src_fn_id INTEGER NOT NULL, dst_fn_id INTEGER NOT NULL, extern INTEGER NOT NULL,
  PRIMARY KEY (src_fn_id, dst_fn_id)) WITHOUT ROWID;
CREATE TABLE call_edge_slow(src_fn_id INTEGER NOT NULL, dst_fn_id INTEGER NOT NULL, extern INTEGER NOT NULL,
  PRIMARY KEY (src_fn_id, dst_fn_id)) WITHOUT ROWID;
INSERT INTO call_edge_fast SELECT DISTINCT s.id, d.id, e.callee_path GLOB '*_rust_*_shim.rs' FROM edge_text AS e
  JOIN fn_text AS s ON s.path = e.caller_path AND s.name = e.caller_name
  JOIN fn_text AS d ON d.path = e.callee_path AND d.name = e.callee_name WHERE e.tier = 'fast';
INSERT INTO call_edge_slow SELECT DISTINCT s.id, d.id, e.callee_path GLOB '*_rust_*_shim.rs' FROM edge_text AS e
  JOIN fn_text AS s ON s.path = e.caller_path AND s.name = e.caller_name
  JOIN fn_text AS d ON d.path = e.callee_path AND d.name = e.callee_name WHERE e.tier = 'slow';
CREATE TABLE seed(anchor TEXT PRIMARY KEY, fn_id INTEGER NOT NULL);
INSERT INTO seed SELECT s.anchor, f.id FROM seed_text AS s JOIN fn_text AS f ON f.path = s.path AND f.name = s.name;
CREATE TABLE t4_pair(tier TEXT NOT NULL, anchor TEXT NOT NULL, seed_fn_id INTEGER NOT NULL, target_fn_id INTEGER NOT NULL,
  PRIMARY KEY (tier, anchor));
ANALYZE;
SQL
  # DuckPGQ rejects WHERE on an edge inside a quantified path, so edge filters are baked into edge tables.
  "$duckdb" "$db/lab-$corpus.duckdb" <<SQL
LOAD sqlite;
ATTACH '$lab' AS src (TYPE sqlite, READ_ONLY);
CREATE TABLE scmpp_dict_path AS SELECT * FROM src.scmpp_dict_path;
CREATE TABLE scmpp_dict_kind AS SELECT * FROM src.scmpp_dict_kind;
CREATE TABLE scmpp_dict_field AS SELECT * FROM src.scmpp_dict_field;
CREATE TABLE scmpp_dict_capture AS SELECT * FROM src.scmpp_dict_capture;
CREATE TABLE scmpp_dict_text AS SELECT * FROM src.scmpp_dict_text;
CREATE TABLE scmpp_node AS SELECT * FROM src.scmpp_node;
CREATE TABLE scmpp_capture AS SELECT * FROM src.scmpp_capture;
CREATE TABLE fn_dict_path AS SELECT * FROM src.fn_dict_path;
CREATE TABLE fn_dict_name AS SELECT * FROM src.fn_dict_name;
CREATE TABLE fn AS SELECT * FROM src.fn;
CREATE TABLE call_edge_fast AS SELECT * FROM src.call_edge_fast;
CREATE TABLE call_edge_slow AS SELECT * FROM src.call_edge_slow;
CREATE TABLE seed AS SELECT * FROM src.seed;
CREATE TABLE cst_vertex AS SELECT file::BIGINT * 4294967296 + pre AS id, file, pre, kind, start, "end" FROM scmpp_node;
CREATE TABLE chain_edge AS SELECT c.file::BIGINT * 4294967296 + c.parent AS src, c.file::BIGINT * 4294967296 + c.pre AS dst
  FROM scmpp_node AS c JOIN scmpp_dict_field AS f ON f.id = c.field WHERE c.parent >= 0 AND f.text IN ('function', 'value');
CREATE TABLE calls_fast AS SELECT src_fn_id, dst_fn_id FROM call_edge_fast WHERE extern = 0;
CREATE TABLE calls_slow AS SELECT src_fn_id, dst_fn_id FROM call_edge_slow WHERE extern = 0;
EXPORT DATABASE '$db/parquet-$corpus' (FORMAT parquet);
SQL
  printf '%s\t%s\t%s\n' "$corpus" fn "$("$sqlite" "$lab" 'SELECT count(*) FROM fn')" >> "$db/export_counts.tsv"
  for table in call_edge_fast call_edge_slow seed scmpp_node; do
    printf '%s\t%s\t%s\n' "$corpus" "$table" "$("$sqlite" "$lab" "SELECT count(*) FROM $table")" >> "$db/export_counts.tsv"
  done
done
