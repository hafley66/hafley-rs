//! Fast vs slow, per call site, over a frozen copy of soopy: `ryi fast` and
//! `ryi slow` (over `fixtures/ratchet_soopy/index.scip`) each write a database
//! and the grade is SQL over the two. Totals are pinned in `tests/RATCHET_SITES.tsv`;
//! `RATCHET_BUMP=1` moves them in the good direction only. `regen.sh` rebuilds
//! the corpus copy and its index.

#![cfg(feature = "cli")]

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

const CORPUS: &str = "tests/fixtures/ratchet_soopy";
static GRADE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Default, Debug, Clone, Copy, PartialEq)]
struct Counts {
    tp: usize,
    wrong_target: usize,
    overbound: usize,
    miss: usize,
    tn: usize,
    no_occurrence: usize,
}

impl Counts {
    fn add(&mut self, class: &str, n: usize) {
        match class {
            "tp" => self.tp += n,
            "wrong_target" => self.wrong_target += n,
            "overbound" => self.overbound += n,
            "miss" => self.miss += n,
            "tn" => self.tn += n,
            "no_occurrence" => self.no_occurrence += n,
            other => panic!("unknown class {other}"),
        }
    }
}

/// `slow.resolved_edge` is the oracle's corpus class; its `unresolved` rows of
/// reason `local` / `external` are the other two. A site slow has no row for
/// is `no_occurrence`.
const GRADE: &str = "
with s as (select _input_path path, span__start st from site),
oracle as (
  select caller_path path, caller_site_start st, 'corpus' class,
         callee_path def_path, callee_start def_start
  from slow.resolved_edge where resolution_origin = 'scip'
  union all
  select path, span__start, reason, null, null from slow.unresolved
  where reason in ('local', 'external')
),
ref as (
  select s.*, o.class, o.def_path, o.def_start
  from s left join oracle o on o.path = s.path and o.st = s.st
),
fe as (
  select caller_path path, caller_site_start st, min(callee_path) cp,
         min(callee_start) cs, min(callee_end) ce, min(resolution_origin) origin
  from resolved_edge group by 1, 2
),
un as (select path, span__start st, min(reason) reason from unresolved group by 1, 2)
select coalesce(fe.origin, 'unresolved:' || un.reason, 'none') bucket,
  case
    when ref.class is null then 'no_occurrence'
    when ref.class = 'local' then case when fe.cp is null then 'tn' else 'overbound' end
    when ref.class = 'corpus' and fe.cp is null then 'miss'
    when ref.class = 'corpus' and fe.cp = ref.def_path
         and ref.def_start >= fe.cs and ref.def_start < fe.ce then 'tp'
    when ref.class = 'corpus' then 'wrong_target'
    when fe.cp is not null then 'overbound'
    else 'tn'
  end cls,
  count(*)
from ref
left join fe on fe.path = ref.path and fe.st = ref.st
left join un on un.path = ref.path and un.st = ref.st
group by 1, 2";

fn ryi(args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(args)
        .env("RUST_LOG", "off")
        .output()
        .expect("run ryi");
    assert!(output.status.success(), "ryi {args:?}: {}", String::from_utf8_lossy(&output.stderr));
}

/// Type edges keyed by (owner, kind, target): both tiers, fast only, slow only.
const TYPE_GRADE: &str = "
with f as (select distinct owner_path, owner_start, kind, target_name, target_path
           from resolved_type_edge),
     s as (select distinct owner_path, owner_start, kind, target_name, target_path
           from slow.resolved_type_edge),
     b as (select * from f intersect select * from s)
select (select count(*) from b),
       (select count(*) from f) - (select count(*) from b),
       (select count(*) from s) - (select count(*) from b)";

/// Both databases over the frozen corpus, fast's opened with slow's attached.
fn tiers() -> (tempfile::TempDir, rusqlite::Connection) {
    let scratch = tempfile::tempdir().expect("scratch dir");
    let fast = scratch.path().join("fast.db");
    let slow = scratch.path().join("slow.db");
    let src = format!("{CORPUS}/src");
    let index = format!("{CORPUS}/index.scip");
    ryi(&["fast", &src, "--sqlite", &fast.to_string_lossy()]);
    ryi(&[
        "slow",
        &src,
        "--root",
        CORPUS,
        "--scip-index",
        &index,
        "--no-checker",
        "--sqlite",
        &slow.to_string_lossy(),
    ]);

    let conn = rusqlite::Connection::open(&fast).expect("open fast.db");
    conn.execute("attach ?1 as slow", [slow.to_string_lossy()]).expect("attach slow.db");
    (scratch, conn)
}

fn grade() -> BTreeMap<String, Counts> {
    let (_scratch, conn) = tiers();
    let mut statement = conn.prepare(GRADE).expect("grade sql");
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)? as usize))
        })
        .expect("grade rows");
    let mut by_bucket: BTreeMap<String, Counts> = BTreeMap::new();
    for row in rows {
        let (bucket, class, n) = row.expect("grade row");
        by_bucket.entry(bucket).or_default().add(&class, n);
    }
    by_bucket
}

#[test]
fn fast_matches_slow_on_soopy_at_the_pinned_rate() {
    let _guard = GRADE_LOCK.lock().unwrap();
    let by_bucket = grade();
    let mut total = Counts::default();
    eprintln!("bucket\ttp\twrong\toverbound\tmiss\ttn\tno_occ");
    for (bucket, c) in &by_bucket {
        eprintln!(
            "{bucket}\t{}\t{}\t{}\t{}\t{}\t{}",
            c.tp, c.wrong_target, c.overbound, c.miss, c.tn, c.no_occurrence
        );
        total.tp += c.tp;
        total.wrong_target += c.wrong_target;
        total.overbound += c.overbound;
        total.miss += c.miss;
        total.tn += c.tn;
        total.no_occurrence += c.no_occurrence;
    }

    let pin_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/RATCHET_SITES.tsv");
    let header = "corpus\ttp\twrong_target\toverbound\tmiss\n";
    let pinned = std::fs::read_to_string(&pin_path).unwrap_or_default();
    let cells: Vec<usize> = pinned
        .lines()
        .nth(1)
        .map(|line| line.split('\t').skip(1).map(|cell| cell.parse().expect("pin cell")).collect())
        .unwrap_or_default();

    if matches!(std::env::var("RATCHET_BUMP").as_deref(), Ok("1")) {
        let [tp, wrong, over, miss] = match cells.as_slice() {
            [tp, wrong, over, miss] => [
                (*tp).max(total.tp),
                (*wrong).min(total.wrong_target),
                (*over).min(total.overbound),
                (*miss).min(total.miss),
            ],
            _ => [total.tp, total.wrong_target, total.overbound, total.miss],
        };
        std::fs::write(&pin_path, format!("{header}soopy\t{tp}\t{wrong}\t{over}\t{miss}\n"))
            .expect("write pin");
        return;
    }

    let [tp, wrong, over, miss] = cells.as_slice() else {
        panic!("tests/RATCHET_SITES.tsv has no soopy row: run once with RATCHET_BUMP=1");
    };
    assert!(total.tp >= *tp, "tp fell: {} < pinned {tp}", total.tp);
    assert!(total.wrong_target <= *wrong, "wrong_target rose: {} > pinned {wrong}", total.wrong_target);
    assert!(total.overbound <= *over, "overbound rose: {} > pinned {over}", total.overbound);
    assert!(total.miss <= *miss, "miss rose: {} > pinned {miss}", total.miss);
}

#[test]
fn fast_type_edges_match_slow_on_soopy_at_the_pinned_rate() {
    let _guard = GRADE_LOCK.lock().unwrap();
    let (_scratch, conn) = tiers();
    let (both, fast_only, slow_only): (usize, usize, usize) = conn
        .query_row(TYPE_GRADE, [], |row| {
            Ok((row.get::<_, i64>(0)? as usize, row.get::<_, i64>(1)? as usize, row.get::<_, i64>(2)? as usize))
        })
        .expect("type grade");
    eprintln!("type edges: both {both}, fast only {fast_only}, slow only {slow_only}");

    let pin_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/RATCHET_TYPES.tsv");
    let pinned = std::fs::read_to_string(&pin_path).unwrap_or_default();
    let cells: Vec<usize> = pinned
        .lines()
        .nth(1)
        .map(|line| line.split('\t').skip(1).map(|cell| cell.parse().expect("pin cell")).collect())
        .unwrap_or_default();
    if matches!(std::env::var("RATCHET_BUMP").as_deref(), Ok("1")) {
        let [b, f, s] = match cells.as_slice() {
            [b, f, s] => [(*b).max(both), (*f).min(fast_only), (*s).min(slow_only)],
            _ => [both, fast_only, slow_only],
        };
        std::fs::write(&pin_path, format!("corpus\tboth\tfast_only\tslow_only\nsoopy\t{b}\t{f}\t{s}\n"))
            .expect("write pin");
        return;
    }
    let [b, f, s] = cells.as_slice() else {
        panic!("tests/RATCHET_TYPES.tsv has no soopy row: run once with RATCHET_BUMP=1");
    };
    assert!(both >= *b, "type edges both fell: {both} < pinned {b}");
    assert!(fast_only <= *f, "fast-only type edges rose: {fast_only} > pinned {f}");
    assert!(slow_only <= *s, "slow-only type edges rose: {slow_only} > pinned {s}");
}

/// `regen.sh` runs this with `RATCHET_INDEX` naming a fresh rust-analyzer index
/// of soopy: the fixture keeps only its `src/` documents, no docstrings.
#[test]
#[ignore]
fn regen_fixture_index() {
    let fresh = std::env::var("RATCHET_INDEX").expect("RATCHET_INDEX names a fresh index.scip");
    let out = Path::new(CORPUS).join("index.scip");
    let kept = sprefa_extract::scip_decode::prune_index(Path::new(&fresh), &out, "src/")
        .expect("prune index");
    eprintln!("kept {kept} documents in {}", out.display());
}
