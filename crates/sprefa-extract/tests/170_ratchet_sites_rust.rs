//! Fast vs slow, per call site, over a frozen copy of soopy: `ryi fast` rows
//! joined to rust-analyzer's SCIP occurrences (`fixtures/ratchet_soopy/oracle.tsv`,
//! rebuilt by `fixtures/ratchet_soopy/regen.sh`). Totals are pinned in
//! `tests/RATCHET_SITES.tsv`; `RATCHET_BUMP=1` moves them in the good direction only.

#![cfg(feature = "cli")]

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

const CORPUS: &str = "tests/fixtures/ratchet_soopy";

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

const GRADE: &str = "
with s as (select _input_path path, span__start st, span__end en from site),
ref as (
  select s.*, o.class, o.def_path, o.def_start
  from s left join oracle o
    on '{CORPUS}/' || o.path = s.path and o.end = s.en and o.start >= s.st
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
    when ref.class = 'corpus' and fe.cp = '{CORPUS}/' || ref.def_path
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

fn grade() -> BTreeMap<String, Counts> {
    let scratch = tempfile::tempdir().expect("scratch dir");
    let db = scratch.path().join("fast.db");
    let mut paths: Vec<String> = walk(Path::new(CORPUS).join("src").as_path());
    paths.sort();
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .arg("fast")
        .arg("--sqlite")
        .arg(&db)
        .args(&paths)
        .env("RUST_LOG", "off")
        .output()
        .expect("run ryi fast");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let conn = rusqlite::Connection::open(&db).expect("open fast.db");
    conn.execute_batch(
        "create table oracle(path text, start integer, end integer, class text, \
         def_path text, def_start integer)",
    )
    .expect("oracle table");
    let oracle = std::fs::read_to_string(Path::new(CORPUS).join("oracle.tsv")).expect("oracle.tsv");
    {
        let mut insert = conn
            .prepare("insert into oracle values (?1, ?2, ?3, ?4, nullif(?5, ''), nullif(?6, ''))")
            .expect("insert");
        for line in oracle.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            insert.execute(rusqlite::params![f[0], f[1], f[2], f[3], f[4], f[5]]).expect("row");
        }
    }
    let sql = GRADE.replace("{CORPUS}", CORPUS);
    let mut statement = conn.prepare(&sql).expect("grade sql");
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

fn walk(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("read corpus dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path.to_string_lossy().into_owned());
        }
    }
    out
}

#[test]
fn fast_matches_slow_on_soopy_at_the_pinned_rate() {
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
