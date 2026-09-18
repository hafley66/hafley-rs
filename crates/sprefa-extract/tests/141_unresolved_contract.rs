//! `unresolved` names its file in ONE column. Before the fix, `path` (phase-2,
//! `call_drop_facts`) and `_input_path` (every SQLite row, phase-1 included)
//! were complementary: a phase-1 miss (`tests/fixtures/ts_unresolved/unresolved.ts`
//! has four) left `path` NULL and only `_input_path` named the file, so
//! `WHERE path = ...` silently dropped a row that knew its own file.
//!
//! The fix stamps `path` at the flatten site in `src/project.rs`
//! (`resolve_project_with_raw`), never in `src/wire.rs`: a phase-1 row's own
//! `flatten()` (pinned dead by `tests/20_unresolved.rs`) keeps `path: None`,
//! since that door has no file to name; a project resolve knows every input's
//! path and now says so.

use rusqlite::Connection;
use std::process::Command;

const PHASE1: &str = "tests/fixtures/ts_unresolved/unresolved.ts";
const DEFS: &str = "tests/fixtures/ts_untyped_receiver/defs.ts";
const USE: &str = "tests/fixtures/ts_untyped_receiver/use.ts";
const TYPE_A: &str = "tests/fixtures/ts/sample.ts";
const TYPE_B: &str = "tests/fixtures/ts/consts.ts";

fn extract(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .expect("extract binary runs")
}

fn ok_stdout(args: &[&str]) -> String {
    let output = extract(args);
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn sqlite_export(db: &std::path::Path, args: &[&str]) {
    let mut full: Vec<&str> = args.to_vec();
    full.push("--sqlite");
    full.push(db.to_str().unwrap());
    let output = extract(&full);
    assert!(
        output.status.success(),
        "{full:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The defect, pinned: a 3-file `fast --sqlite` export mixes PHASE1's four
/// per-file misses with the phase-2 drops `DEFS`+`USE` trigger (Lane D,
/// `tests/136_untyped_receiver_ts.rs`), and no row may leave `path` NULL.
#[test]
fn no_unresolved_row_leaves_its_file_column_null() {
    let scratch = tempfile::tempdir().unwrap();
    let db = scratch.path().join("mixed.db");
    sqlite_export(&db, &["fast", PHASE1, DEFS, USE]);
    let conn = Connection::open(&db).unwrap();
    let total: i64 = conn
        .query_row("SELECT count(*) FROM unresolved", [], |r| r.get(0))
        .unwrap();
    let null_path: i64 = conn
        .query_row(
            "SELECT count(*) FROM unresolved WHERE path IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(total >= 7, "expected phase-1 and phase-2 rows both: {total}");
    assert_eq!(null_path, 0, "every unresolved row must name its file");
}

/// Today the stdout `--resolve` stream and the SQLite table differ by however
/// many phase-1 rows the input set carries, and nothing catches it. `_input_path`
/// stays the phase-2 discriminator (only `source_fact`, the phase-1 sink, ever
/// sets it) even after the `path` stamp, so it isolates the same rows stdout
/// carries.
#[test]
fn stream_and_table_agree_on_phase_two_unresolved_rows() {
    let scratch = tempfile::tempdir().unwrap();
    let db = scratch.path().join("agree.db");
    sqlite_export(&db, &["fast", PHASE1, DEFS, USE]);
    let stream = ok_stdout(&["--resolve", "--family", "call", PHASE1, DEFS, USE]);
    let stream_count = stream
        .lines()
        .filter(|line| line.contains(r#""record":"unresolved""#))
        .count();
    let conn = Connection::open(&db).unwrap();
    let table_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM unresolved WHERE _input_path IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(stream_count > 0, "the phase-2 drops must fire: {stream}");
    assert_eq!(table_count, stream_count as i64);
}

/// The stamp adds a column value and changes nothing else: same reason, same
/// span, same detail as the hand-derived receipt in `tests/20_unresolved.rs`,
/// plus its own file where the row previously said nothing.
#[test]
fn phase_one_rows_keep_reason_and_span() {
    let scratch = tempfile::tempdir().unwrap();
    let db = scratch.path().join("stamped.db");
    sqlite_export(&db, &["fast", PHASE1, DEFS, USE]);
    let conn = Connection::open(&db).unwrap();
    let (path, detail, start, end): (String, String, i64, i64) = conn
        .query_row(
            "SELECT path, detail, span__start, span__end FROM unresolved \
             WHERE reason = 'spread-call-args'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(path, PHASE1);
    assert_eq!(detail, "...args");
    assert_eq!((start, end), (73, 80));
}

/// Pins the behavior a future reader might "fix" after misreading the old
/// `--help` text: `--resolve --sqlite` (family defaults to `call` alone)
/// never emits `resolved_type_edge`; `fast --sqlite` (`--family diet_scip`
/// rewrites to call+type) does.
#[test]
fn mode_flag_still_honored_after_the_fix() {
    let scratch = tempfile::tempdir().unwrap();
    let resolve_db = scratch.path().join("resolve.db");
    let fast_db = scratch.path().join("fast.db");
    sqlite_export(&resolve_db, &["--resolve", TYPE_A, TYPE_B]);
    sqlite_export(&fast_db, &["fast", TYPE_A, TYPE_B]);
    let resolve_count: i64 = Connection::open(&resolve_db)
        .unwrap()
        .query_row("SELECT count(*) FROM resolved_type_edge", [], |r| r.get(0))
        .unwrap();
    let fast_count: i64 = Connection::open(&fast_db)
        .unwrap()
        .query_row("SELECT count(*) FROM resolved_type_edge", [], |r| r.get(0))
        .unwrap();
    assert_eq!(resolve_count, 0, "--resolve alone stays call-only");
    assert!(fast_count > 0, "fast mode's diet_scip rewrite adds type");
}
