#!/usr/bin/env bash
# usage: 2_scmpp.sh [small|crates ...]  -> arm B: scm++ self-call (T2) and fixed-depth chains (T1); B_depth<k> = union of depths 1..k, wall summed
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source "$here/0_measure.sh"
repo=$(cd -- "$here/../../../../.." && pwd)
ryii=${RYII:-$repo/crates/sprefa-extract/target/release/ryii}
sqlite=${SQLITE:-/opt/homebrew/opt/sqlite/bin/sqlite3}
mkdir -p "$here/db/b"
cd "$repo"
for corpus in ${@:-small crates}; do
  case $corpus in
    small) inputs=(crates/hafley_scm/src crates/sprefa-extract/src) ;;
    crates) inputs=(crates) ;;
  esac
  store=$here/db/b/self_call-$corpus.db
  rm -f "$store"
  measure B T2 "$corpus" cst 1 0 "$(wc -l < "$here/2_self_call.scm" | tr -d ' ')" "B_T2_${corpus}_cst" -- \
    "$ryii" query --scmpp "$here/2_self_call.scm" --sqlite "$store" --pattern '*.rs' "${inputs[@]}"
  "$sqlite" -header -separator $'\t' "$store" \
    "SELECT DISTINCT path, item__start AS fn_start, item__end AS fn_end FROM scmpp_row" > "$here/db/out/B_T2_${corpus}_cst.tsv"
  wall=0
  for depth in 1 2 3 4 5; do
    store=$here/db/b/chain_depth$depth-$corpus.db
    rm -f "$store"
    out=B_depth${depth}_T1_${corpus}_cst
    measure "B_depth$depth" T1 "$corpus" cst 1 0 "$(wc -l < "$here/2_chain_depth$depth.scm" | tr -d ' ')" "$out" -- \
      "$ryii" query --scmpp "$here/2_chain_depth$depth.scm" --sqlite "$store" --pattern '*.rs' "${inputs[@]}"
    union=$(for d in $(seq 1 "$depth"); do
      printf "SELECT path, outer__start, outer__end, link__start, link__end FROM d$d.scmpp_row UNION "; done)
    attach=$(for d in $(seq 1 "$depth"); do printf "ATTACH '%s' AS d%s; " "$here/db/b/chain_depth$d-$corpus.db" "$d"; done)
    "$sqlite" -header -separator $'\t' :memory: "$attach" \
      "SELECT path, outer__start AS outer_start, outer__end AS outer_end, link__start AS link_start, link__end AS link_end FROM (${union% UNION })" \
      > "$here/db/out/$out.tsv"
    # wall and notation lines of B_depth<k> are the sums over the depth files 1..k
    python3 - "$here/db/runs.tsv" "$out" "$depth" "$corpus" <<'PY'
import os, sys
path, out, depth, corpus = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
lines = open(path).read().splitlines()
head = lines[0].split("\t")
rows = [line.split("\t") for line in lines[1:]]
level = lambda d: f"{os.path.dirname(path)}/out/B_depth{d}_T1_{corpus}_cst.err"
wall = lambda d: float(next(line.split()[0] for line in open(level(d)) if " real " in line))
target = next(r for r in reversed(rows) if r[head.index("output")] == out)
target[head.index("wall_s")] = f"{sum(wall(d) for d in range(1, depth + 1)):.2f}"
target[head.index("notation_lines")] = str(sum(int(os.popen(f"wc -l < {os.path.dirname(os.path.dirname(path))}/2_chain_depth{d}.scm").read()) for d in range(1, depth + 1)))
target[head.index("note")] = f"union of fixed-depth files 1..{depth}; wall summed"
open(path, "w").write("\n".join([lines[0]] + ["\t".join(r) for r in rows]) + "\n")
PY
  done
done
