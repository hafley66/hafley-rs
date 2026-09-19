#!/usr/bin/env bash
# usage: typegraph-d2.sh [MIN_DEGREE] [OUT.d2]
set -euo pipefail

ROOT=${ROOT:-$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)}
MIN_DEG=${1:-4}
OUT=${2:-$ROOT/crates/boop/plans/2026-09-19-boop-type-graph.d2}
DB=${DB:-${TMPDIR:-/tmp}/boop-typegraph.db}
CRATES=${CRATES:-"boop boop-acp boop-harness boop-mux boop-proc boop-store boop-turnstrip boop-turnvis"}

say() { printf '\033[38;5;80m%s\033[0m\n' "$*" >&2; }

# ---------------------------------------------------------------- 1. sources
say "1. collecting non-test sources across: $CRATES"
FILES=$(mktemp)
for c in $CRATES; do
  find "$ROOT/crates/$c/src" -name '*.rs' -not -path '*/target/*' 2>/dev/null || true
done | sort -u > "$FILES"
say "   $(wc -l < "$FILES" | tr -d ' ') files (src/ only; tests/ benches/ examples/ never enter)"

# ---------------------------------------------------------------- 2. extract
say "2. extract --family cst --sqlite"
rm -f "$DB"
# shellcheck disable=SC2046
extract --family cst --sqlite "$DB" $(tr '\n' ' ' < "$FILES") >/dev/null
say "   $(sqlite3 "$DB" 'SELECT COUNT(*) FROM node')' nodes"

# ---------------------------------------------------------------- 3. tiers
say "3. sqlite: declarations, edges, longest-path tiers"
sqlite3 "$DB" <<'SQL'
CREATE INDEX IF NOT EXISTS ix_node_k ON node(kind,_input_path,span__start,span__end);

-- every `mod test` / `mod tests` span. Anything inside one is invisible.
DROP TABLE IF EXISTS test_span;
CREATE TABLE test_span AS
  SELECT _input_path AS path, span__start AS s, span__end AS e
  FROM node WHERE kind='mod_item' AND lower(name) IN ('test','tests');

DROP TABLE IF EXISTS ty_decl;
CREATE TABLE ty_decl AS
  SELECT n.name AS ty, n.kind AS kind, n._input_path AS path,
         n.span__start AS s, n.span__end AS e
  FROM node n
  WHERE n.kind IN ('struct_item','enum_item','type_item','union_item','trait_item')
    AND NOT EXISTS (SELECT 1 FROM test_span t
                    WHERE t.path=n._input_path AND n.span__start>=t.s AND n.span__end<=t.e);
CREATE INDEX ix_decl ON ty_decl(ty);

DROP TABLE IF EXISTS ty_edge;
CREATE TABLE ty_edge AS
  SELECT DISTINCT d.ty AS src, r.name AS dst
  FROM ty_decl d
  JOIN node r ON r.kind='type_identifier' AND r._input_path=d.path
             AND r.span__start>=d.s AND r.span__end<=d.e AND r.name<>d.ty
  WHERE r.name IN (SELECT ty FROM ty_decl)
    AND NOT EXISTS (SELECT 1 FROM test_span t
                    WHERE t.path=r._input_path AND r.span__start>=t.s AND r.span__end<=t.e);
CREATE INDEX ix_edge ON ty_edge(src,dst);

-- tier = longest path to a sink. A leaf is tier 0; a type sits one above its
-- deepest callee. The graph is a DAG (420 singleton SCCs), so this terminates.
DROP TABLE IF EXISTS tier;
CREATE TABLE tier AS
WITH RECURSIVE depth(ty,d) AS (
  SELECT ty, 0 FROM ty_decl WHERE ty NOT IN (SELECT src FROM ty_edge)
  UNION
  SELECT e.src, depth.d+1 FROM ty_edge e JOIN depth ON depth.ty=e.dst WHERE depth.d < 12
)
SELECT ty, MAX(d) AS t FROM depth GROUP BY ty;
CREATE INDEX ix_tier ON tier(ty);

DROP TABLE IF EXISTS deg;
CREATE TABLE deg AS
  SELECT ty,
         (SELECT COUNT(*) FROM ty_edge WHERE dst=ty_decl.ty) AS ind,
         (SELECT COUNT(*) FROM ty_edge WHERE src=ty_decl.ty) AS outd
  FROM (SELECT DISTINCT ty FROM ty_decl) ty_decl;
CREATE INDEX ix_deg ON deg(ty);
SQL

sqlite3 -header -column "$DB" \
  "SELECT (SELECT COUNT(DISTINCT ty) FROM ty_decl) types,
          (SELECT COUNT(*) FROM ty_edge) edges,
          (SELECT MAX(t) FROM tier) max_tier" >&2

# ---------------------------------------------------------------- 4. select
say "4. keeping types with degree >= $MIN_DEG"
sqlite3 -separator '|' "$DB" "
  SELECT d.ty, t.t,
         replace(substr(d2.path, instr(d2.path,'crates/')+7), '/'||substr(d2.path, instr(d2.path,'/src/')+1), '')
  FROM deg d JOIN tier t ON t.ty=d.ty
  JOIN (SELECT ty, MIN(path) path FROM ty_decl GROUP BY ty) d2 ON d2.ty=d.ty
  WHERE d.ind + d.outd >= $MIN_DEG
  ORDER BY t.t, 3, d.ty" > "${DB%.db}.nodes"

cut -d'|' -f1 "${DB%.db}.nodes" | sort > "${DB%.db}.keep"
sqlite3 -separator '|' "$DB" "SELECT src, dst FROM ty_edge" \
  | awk -F'|' 'NR==FNR{k[$0];next} ($1 in k)&&($2 in k)' "${DB%.db}.keep" - > "${DB%.db}.edges"

say "   $(wc -l < "${DB%.db}.nodes" | tr -d ' ') shapes, $(wc -l < "${DB%.db}.edges" | tr -d ' ') edges"

# ---------------------------------------------------------------- 5. d2
say "5. emitting d2 -> $OUT"
mkdir -p "$(dirname "$OUT")"
awk -F'|' -v edges="${DB%.db}.edges" -v crates="$CRATES" '
function key(s){gsub(/[^A-Za-z0-9_]/,"_",s); return s}
BEGIN{
  n=split(crates,C," ")
  # 8-color categorical palette, one per crate, each with its own font-color
  fill["boop"]="#1f4f82";           fg["boop"]="#eaf2fb"
  fill["boop-store"]="#1d6b4f";     fg["boop-store"]="#e8f7ef"
  fill["boop-proc"]="#7a3d8f";      fg["boop-proc"]="#f6ecfa"
  fill["boop-harness"]="#a8531c";   fg["boop-harness"]="#fdf0e6"
  fill["boop-mux"]="#146b74";       fg["boop-mux"]="#e6f6f8"
  fill["boop-acp"]="#8f2f3f";       fg["boop-acp"]="#fbebee"
  fill["boop-turnstrip"]="#5b5f22"; fg["boop-turnstrip"]="#f4f5e6"
  fill["boop-turnvis"]="#2f3f8f";   fg["boop-turnvis"]="#eceffb"
}
{ ty=$1; t=$2; cr=$3; if(!(cr in fill)) cr="boop"
  tiers[t]=1; crate[ty]=cr; tierof[ty]=t
  members[t]=members[t] " " ty
  ntypes[cr]++ }
END{
  print "# generated by crates/boop/scripts/typegraph-d2.sh -- do not hand-edit"
  print "vars: {"
  print "  d2-config: {"
  print "    layout-engine: elk"
  print "    theme-id: 200"
  print "    pad: 30"
  print "    center: true"
  print "  }"
  print "}"
  print "direction: down"
  print ""
  print "classes: {"
  for(c in fill) if(ntypes[c]>0){
    print "  c_" key(c) ": { style: { fill: \"" fill[c] "\"; stroke: \"#0b0d10\"; font-color: \"" fg[c] "\"; border-radius: 6; stroke-width: 1 } }"
  }
  print "}"
  print ""

  # Tier containers are wrong here: the ranks the engine computes already ARE
  # the tiers, and the boxes only add gutters plus forced edge detours.
  for(t=0;t<=64;t++){
    if(!(t in tiers)) continue
    split(members[t],M," ")
    for(i in M){ ty=M[i]; if(ty=="") continue
      printf "%s: \"%s\" { class: c_%s }\n", key(ty), ty, key(crate[ty]) }
  }
  print ""

  # ---- edges, colored by the SOURCE crate ------------------------------
  print "# edge stroke = the crate doing the pointing."
  print "# solid = same crate, dashed = crosses a crate boundary,"
  print "# thicker = skips a tier (a candidate for the wrong stratum)."
  while((getline line < edges) > 0){
    split(line,E,"|"); a=E[1]; b=E[2]
    if(!(a in crate) || !(b in crate)) continue
    ca=crate[a]; cb=crate[b]
    pa=key(a); pb=key(b)
    jump=tierof[a]-tierof[b]
    w=(jump>1?3:1)
    dash=(ca==cb ? 0 : 4)
    printf "%s -> %s: { style: { stroke: \"%s\"; stroke-width: %d; stroke-dash: %d; opacity: 0.85 } }\n", pa, pb, fill[ca], w, dash
  }
  close(edges)

  # ---- second board: crate distribution as tables, no edges so grid is legal
  print ""
  print "layers: {"
  print "  crates: {"
  print "    direction: down"
  print "    grid-columns: 3"
  print "    grid-gap: 24"
  for(c in fill){
    if(ntypes[c]<1) continue
    printf "    w_%s: \"%s\" {\n", key(c), c
    printf "      style: { fill: \"%s\"; stroke: \"#0b0d10\"; font-color: \"%s\"; border-radius: 8 }\n", fill[c], fg[c]
    printf "      tbl: \"%d types\" {\n", ntypes[c]
    print  "        shape: sql_table"
    for(t=0;t<=64;t++){
      if(!(t in tiers)) continue
      split(members[t],M," ")
      for(i in M){ ty=M[i]; if(ty=="" || crate[ty]!=c) continue
        printf "        %s: tier %s\n", ty, t }
    }
    print  "      }"
    print  "    }"
  }
  print "  }"
  print "}"
}' "${DB%.db}.nodes" > "$OUT"

# ---------------------------------------------------------------- 6. gate
say "6. compile gate"
if command -v d2 >/dev/null; then
  GATE=${TMPDIR:-/tmp}/typegraph-gate.svg
  rm -rf "${GATE%.svg}"
  d2 --layout=elk "$OUT" "$GATE" >/dev/null 2>&1 && say "   compiles"
  for b in "${GATE%.svg}"/*.svg; do
    [ -f "$b" ] || continue
    VB=$(grep -o 'viewBox="[^"]*"' "$b" | head -1)
    W=$(echo "$VB" | awk -F'[ "]' '{print $4}'); H=$(echo "$VB" | awk -F'[ "]' '{print $5}')
    say "   $(basename "$b" .svg)  $VB  aspect=$(awk -v w="$W" -v h="$H" 'BEGIN{printf "%.2f", w/h}')"
  done
else
  say "   d2 absent, skipping compile gate"
fi
printf 'shapes: %s\nedges:  %s\nfile:   %s\n' \
  "$(wc -l < "${DB%.db}.nodes" | tr -d ' ')" \
  "$(wc -l < "${DB%.db}.edges" | tr -d ' ')" "$OUT"
