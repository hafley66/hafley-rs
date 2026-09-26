#!/usr/bin/env bash
# usage: [SCIP=index.scip] [TYPE=Name] ryi-typegraph.sh [ROOT] [SRC]
set -uo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
ROOT=$(cd "${1:-$here/tests/fixtures/ratchet_soopy}" && pwd)
SRC=${2:-$ROOT/src}
SCIP=${SCIP:-$([ -f "$ROOT/index.scip" ] && echo "$ROOT/index.scip")}
OUT=$ROOT/.dl/.state/typegraph
mkdir -p "$OUT" && rm -f "$OUT"/{fast,slow,scip}.db
export RUST_LOG=off
index=(${SCIP:+--scip-index "$SCIP"})

time ryi fast "$SRC" --sqlite "$OUT/fast.db" || exit 1
time ryi slow "$SRC" --root "$ROOT" "${index[@]}" --sqlite "$OUT/slow.db" || exit 1
time ryi scip "$SRC" --root "$ROOT" "${index[@]}" --sqlite "$OUT/scip.db" || exit 1

q() { sqlite3 -header -column "$OUT/fast.db" \
  "attach '$OUT/slow.db' as slow; attach '$OUT/scip.db' as scip; $1" | sed "s#$ROOT/##g"; }
h() { printf '\n== %s\n' "$1"; }

h "rows per table: fast | slow | scip"
for db in fast slow scip; do
  sqlite3 "$OUT/$db.db" "select name from sqlite_master where type='table'" |
    while read -r t; do
      printf '%s\t%s\t%s\n' "$t" "$db" "$(sqlite3 "$OUT/$db.db" "select count(*) from \"$t\"")"
    done
done | awk -F'\t' '{n[$1, $2] = $3; t[$1] = 1} END {
  printf "%-28s %8s %8s %8s\n", "table", "fast", "slow", "scip"
  for (k in t) printf "%-28s %8s %8s %8s\n", k, n[k, "fast"], n[k, "slow"], n[k, "scip"]
}' | sort

h "type edges keyed (owner, kind, target): both / fast only / slow only, by kind"
q "with f as (select distinct owner_path, owner_start, kind, target_name, target_path from resolved_type_edge),
       s as (select distinct owner_path, owner_start, kind, target_name, target_path from slow.resolved_type_edge),
       b as (select * from f intersect select * from s)
   select kind,
          (select count(*) from b where b.kind = k.kind) both,
          (select count(*) from f where f.kind = k.kind) - (select count(*) from b where b.kind = k.kind) fast_only,
          (select count(*) from s where s.kind = k.kind) - (select count(*) from b where b.kind = k.kind) slow_only
   from (select kind from f union select kind from s) k order by 1"

h "type edges only one tier has (first 15 each)"
q "select 'fast' tier, * from (select distinct owner_path, owner_start, kind, target_name, target_path from resolved_type_edge
     except select distinct owner_path, owner_start, kind, target_name, target_path from slow.resolved_type_edge) limit 15"
q "select 'slow' tier, * from (select distinct owner_path, owner_start, kind, target_name, target_path from slow.resolved_type_edge
     except select distinct owner_path, owner_start, kind, target_name, target_path from resolved_type_edge) limit 15"

h "call sites graded against slow (tests/170_ratchet_sites_rust.rs GRADE)"
q "with s as (select _input_path path, span__start st from site),
oracle as (
  select caller_path path, caller_site_start st, 'corpus' class, callee_path def_path, callee_start def_start
  from slow.resolved_edge where resolution_origin = 'scip'
  union all
  select path, span__start, reason, null, null from slow.unresolved where reason in ('local', 'external')),
ref as (select s.*, o.class, o.def_path, o.def_start from s left join oracle o on o.path = s.path and o.st = s.st),
fe as (select caller_path path, caller_site_start st, min(callee_path) cp, min(callee_start) cs,
              min(callee_end) ce, min(resolution_origin) origin from resolved_edge group by 1, 2)
select case
    when ref.class is null then 'no_occurrence'
    when ref.class = 'local' then case when fe.cp is null then 'tn' else 'overbound' end
    when ref.class = 'corpus' and fe.cp is null then 'miss'
    when ref.class = 'corpus' and fe.cp = ref.def_path and ref.def_start >= fe.cs and ref.def_start < fe.ce then 'tp'
    when ref.class = 'corpus' then 'wrong_target'
    when fe.cp is not null then 'overbound'
    else 'tn' end cls, count(*) n
from ref left join fe on fe.path = ref.path and fe.st = ref.st group by 1 order by 2 desc"

h "most-referenced types (distinct owners), fast vs slow"
q "select coalesce(f.t, s.t) type, f.n fast, s.n slow from
   (select target_name t, count(distinct owner_path || ':' || owner_start) n from resolved_type_edge group by 1) f
   full join (select target_name t, count(distinct owner_path || ':' || owner_start) n from slow.resolved_type_edge group by 1) s
   on f.t = s.t order by coalesce(f.n, 0) + coalesce(s.n, 0) desc limit 20"

# One type's users in both tiers, owner byte offset turned into file:line.
users() {
  sqlite3 -separator $'\t' "$OUT/fast.db" "attach '$OUT/slow.db' as slow;
    select group_concat(tier, '+'), owner_path, owner_start, kind from (
      select distinct 'fast' tier, owner_path, owner_start, kind from resolved_type_edge where target_name = '$1'
      union select distinct 'slow', owner_path, owner_start, kind from slow.resolved_type_edge where target_name = '$1')
    group by 2, 3, 4 order by 2, 3" |
  while IFS=$'\t' read -r tiers path start kind; do
    file=$path; [ -f "$file" ] || file=$ROOT/$path; [ -f "$file" ] || file=$SRC/$path
    line=$([ -f "$file" ] && head -c "$start" "$file" | wc -l | awk '{print $1 + 1}')
    printf '%-10s %-8s %s:%s  %s\n' "$tiers" "$kind" "${path#"$ROOT"/}" "${line:-?}" \
      "$([ -f "$file" ] && sed -n "${line}p" "$file" | sed 's/^ *//' | cut -c1-90)"
  done
}

pick=${TYPE:-}
if [ -z "$pick" ] && [ -t 0 ] && command -v fzf >/dev/null; then
  pick=$(sqlite3 "$OUT/fast.db" "attach '$OUT/slow.db' as slow;
    select target_name from resolved_type_edge union select target_name from slow.resolved_type_edge order by 1" |
    fzf --prompt 'type> ' --preview "$(declare -f users); OUT='$OUT' ROOT='$ROOT' SRC='$SRC' users {}")
fi
[ -z "$pick" ] && pick=$(sqlite3 "$OUT/fast.db" "select target_name from resolved_type_edge group by 1 order by count(*) desc limit 1")
h "users of $pick (tier: fast, slow, or fast+slow)"
users "$pick"

printf '\nopen: sqlite3 %s -cmd "attach '"'"'%s'"'"' as slow" -cmd "attach '"'"'%s'"'"' as scip"\n' \
  "$OUT/fast.db" "$OUT/slow.db" "$OUT/scip.db"
