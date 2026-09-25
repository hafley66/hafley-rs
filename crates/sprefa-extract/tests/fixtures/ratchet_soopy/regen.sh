#!/usr/bin/env bash
# Rebuild the frozen soopy corpus and its SCIP oracle. Heavy: runs rust-analyzer.
# Run from crates/sprefa-extract.
set -euo pipefail
here=tests/fixtures/ratchet_soopy
soopy=../soopy
work=$(mktemp -d)

rm -rf "$here/src"
cp -R "$soopy/src" "$here/src"
rust-analyzer scip "$soopy" --output "$work/index.scip"
ryi --scip-facts --scip-index "$work/index.scip" --project-root "$PWD/$soopy" \
  --scip-record scip_occurrence --sqlite "$work/occ.db" "$soopy/src/lib.rs"

sqlite3 -separator $'\t' "$work/occ.db" "
create temp table d as
  select symbol, min(path) path, min(start) start from scip_occurrence
  where definition = 1 and path like 'src/%' group by symbol;
select r.path, r.start, r.\"end\",
  case when r.symbol like 'local %' then 'local'
       when d.symbol is not null then 'corpus' else 'external' end,
  coalesce(d.path, ''), coalesce(d.start, '')
from scip_occurrence r left join d on d.symbol = r.symbol
where r.definition = 0 and r.path like 'src/%'
  and (r.symbol like '%().' or r.symbol like '%#' or r.symbol like 'local %')
order by 1, 2, 3" > "$here/oracle.tsv"
rm -rf "$work"
