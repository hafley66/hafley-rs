//! The SCIP ratchet with the scm rows as an input: the fast per-origin
//! histogram before, and the scm-sourced edges joined onto it after.

#![cfg(feature = "cli")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::Value;
use sprefa_extract::{scip_scm_edges, ResolutionOrigin};

const ROOT: &str = "tests/fixtures/ts";

/// The checked-in `ts` floors this lane must not move.
const TS_FLOORS: [(&str, usize); 3] = [("corpus_unique", 8), ("receiver", 1), ("scip", 2)];

/// The scm join gets its own lang key. `pin_ratchet_tsv` drives its walk from
/// every pinned row of a lang, and `call_resolve_scip_ratchet_ts` emits no
/// scm_scope origin, so a `ts` row here would fail that ratchet at once.
const SCM_LANG: &str = "ts_scm";

/// One edge under the judge key: (caller path, caller name, callee path,
/// callee name).
type Key = (String, String, String, String);

#[test]
fn the_checked_in_ts_floors_are_where_this_lane_found_them() {
    let rows = ratchet_rows();
    for (origin, floor) in TS_FLOORS {
        let row = rows
            .iter()
            .find(|row| row.0 == "ts" && row.1 == origin)
            .unwrap_or_else(|| panic!("tests/RATCHET.tsv lost its ts/{origin} row"));
        assert_eq!(
            (row.2, row.3, row.4),
            (floor, 0, 0),
            "ts/{origin} moved: the scm rows are an input, never a replacement"
        );
    }
}

#[test]
fn the_scm_edges_join_onto_the_fast_edges_under_their_own_origin() {
    let paths = ts_files();
    let fast = fast_edges(&paths);
    let scm: BTreeSet<Key> = scm_edges(&paths);

    let mut before: BTreeMap<String, usize> = BTreeMap::new();
    let mut after: BTreeMap<String, usize> = BTreeMap::new();
    for (key, origin) in &fast {
        *before.entry(origin.clone()).or_default() += 1;
        if scm.contains(key) {
            *after.entry(origin.clone()).or_default() += 1;
        }
    }
    let shared = fast.keys().filter(|key| scm.contains(*key)).count();
    // @eprintln-ok: the two histograms this phase reports.
    eprintln!("ratchet over {ROOT}, fast edges by origin (before): {before:?}");
    eprintln!("the same edges the scm graph also names (after): {after:?}");
    eprintln!(
        "scm={} fast={} shared={shared}",
        scm.len(),
        fast.len()
    );

    assert_eq!(
        ResolutionOrigin::ScmScope.as_str(),
        "scm_scope",
        "the new leg has its own closed spelling"
    );
    pin(
        SCM_LANG,
        &BTreeMap::from([(ResolutionOrigin::ScmScope.as_str().to_string(), (shared, 0, 0))]),
    );
}

/// ONE FILE PER RUN, unioned: the lab's recorded TypeScript method. Its first
/// whole-corpus run hit the 10-second limit inside the recursive walk, and a
/// run of these 11 files reproduces that, so the corpus is never one graph.
fn scm_edges(paths: &[PathBuf]) -> BTreeSet<Key> {
    let mut edges = BTreeSet::new();
    for path in paths {
        let started = Instant::now();
        let file = scip_scm_edges(std::slice::from_ref(path)).expect("the scm scope graph resolves");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{}: the scm resolve hit the 10-second limit",
            path.display()
        );
        edges.extend(file.into_iter().map(|edge| {
            (
                edge.caller_path,
                edge.caller_name,
                edge.callee_path,
                edge.callee_name,
            )
        }));
    }
    edges
}

fn ts_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect(Path::new(ROOT), &mut files);
    files.sort();
    files
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).expect("fixture directory") {
        let path = entry.expect("fixture entry").path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("ts") {
            files.push(path);
        }
    }
}

/// `ryi fast` over the same file set, through the binary.
fn fast_edges(paths: &[PathBuf]) -> BTreeMap<Key, String> {
    let trace = std::env::temp_dir().join(format!("ryi-161-{}.json", std::process::id()));
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["--family", "diet_scip"])
        .args(paths)
        .env("HAFLEY_TRACE", trace)
        .env("RUST_LOG", "sprefa_extract=debug")
        .output()
        .expect("ryi runs");
    assert!(
        output.status.success(),
        "ryi fast failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("ryi emits UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("ryi emits JSON"))
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or_default().to_string();
            (
                (
                    text("caller_path"),
                    text("caller_name"),
                    text("callee_path"),
                    text("callee_name"),
                ),
                text("resolution_origin"),
            )
        })
        .collect()
}

type RatchetRow = (String, String, usize, usize, usize);

fn ratchet_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/RATCHET.tsv")
}

fn ratchet_rows() -> Vec<RatchetRow> {
    std::fs::read_to_string(ratchet_path())
        .expect("tests/RATCHET.tsv")
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let cells: Vec<&str> = line.split('\t').collect();
            assert_eq!(cells.len(), 5, "RATCHET.tsv row needs 5 columns: {line}");
            let cell = |index: usize| cells[index].parse::<usize>().expect("RATCHET.tsv cell");
            (
                cells[0].to_string(),
                cells[1].to_string(),
                cell(2),
                cell(3),
                cell(4),
            )
        })
        .collect()
}

/// `tests/RATCHET.tsv`'s own law, applied to this lang's rows: `true` a floor,
/// the other two ceilings, and only `RATCHET_BUMP=1` writes.
fn pin(lang: &str, by_origin: &BTreeMap<String, (usize, usize, usize)>) {
    let mut rows = ratchet_rows();
    if matches!(std::env::var("RATCHET_BUMP").as_deref(), Ok("1")) {
        for (origin, (t, w, u)) in by_origin {
            match rows.iter().position(|row| row.0 == lang && row.1 == *origin) {
                Some(index) => {
                    rows[index].2 = rows[index].2.max(*t);
                    rows[index].3 = rows[index].3.min(*w);
                    rows[index].4 = rows[index].4.min(*u);
                }
                None => rows.push((lang.to_string(), origin.clone(), *t, *w, *u)),
            }
        }
        rows.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        let mut out = String::from("lang\torigin\ttrue\twrong_target\tunresolved\n");
        for (lang, origin, t, w, u) in &rows {
            out.push_str(&format!("{lang}\t{origin}\t{t}\t{w}\t{u}\n"));
        }
        std::fs::write(ratchet_path(), out).expect("write RATCHET.tsv");
        return;
    }
    for origin in by_origin.keys() {
        assert!(
            rows.iter().any(|row| row.0 == lang && row.1 == *origin),
            "unpinned histogram row ({lang}, {origin}): run once with RATCHET_BUMP=1"
        );
    }
    for (_, origin, floor, wrong_ceiling, unresolved_ceiling) in
        rows.iter().filter(|row| row.0 == lang)
    {
        let (t, w, u) = by_origin.get(origin).copied().unwrap_or_default();
        assert!(
            t >= *floor,
            "{lang}/{origin}: true {t} below the pinned floor {floor}"
        );
        assert!(
            w <= *wrong_ceiling,
            "{lang}/{origin}: wrong_target {w} above the pinned ceiling {wrong_ceiling}"
        );
        assert!(
            u <= *unresolved_ceiling,
            "{lang}/{origin}: unresolved {u} above the pinned ceiling {unresolved_ceiling}"
        );
    }
}
