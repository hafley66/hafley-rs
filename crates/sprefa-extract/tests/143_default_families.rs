//! The default `--family` set: derived from the language's own planes, never
//! a list. `ryi FILE` with no `--family` streams the language's non-cst planes
//! when it has any, and the syntax tree when it has nothing else. That second
//! arm covers a cst-only language and a file whose native parse failed alike,
//! so the default can never collapse into an empty stream.
//!
//! FAIL-PRE-FIX RECEIPT at 252a7346: the default was `FamilyMask::ALL`, so the
//! ts fixture below streamed 226 rows of which 171 carried `"family":"cst"`,
//! and the python fixture streamed 288 rows of which 211 were cst. The old
//! coverage table called python "cst only"; its own `Source` fills type, call
//! and df, so python derives exactly like ts.
//!
//! `--family` passed explicitly replaces the default entirely; a cst-only
//! language's default is byte-identical to `--family cst`, which is the
//! receipt that those languages did not move.

use std::path::PathBuf;
use std::process::Command;

const RYI: &str = env!("CARGO_BIN_EXE_ryi");

/// One stdout stream, trail off so a fixture run never touches `~/.agent`.
fn run(args: &[&str]) -> String {
    let out = Command::new(RYI)
        .args(args)
        .env("DL_TRAIL", "0")
        .output()
        .expect("ryi runs");
    assert!(
        out.status.success(),
        "ryi {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("stdout is utf8")
}

fn scratch(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("sprefa-extract-default-families");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(name);
    std::fs::write(&path, body).expect("scratch write");
    path
}

/// A language whose own planes answered must not stream the syntax tree.
#[test]
fn a_full_language_defaults_to_its_own_planes() {
    let rows = run(&["tests/fixtures/ts_checker/src/main.ts"]);
    assert!(
        rows.contains("\"family\":\"df\""),
        "the default should keep the planes the language owns: {rows}"
    );
    assert!(
        !rows.contains("\"family\":\"cst\""),
        "the default must not stream the syntax tree for a language with its own planes: {rows}"
    );
}

/// The coverage table used to file python under "cst only, same route"; its
/// Source fills type, call and df, so the derivation must treat it like ts.
#[test]
fn python_defaults_to_its_own_planes_too() {
    let rows = run(&["tests/fixtures/python/sample.py"]);
    assert!(rows.contains("\"family\":\"type\""), "{rows}");
    assert!(!rows.contains("\"family\":\"cst\""), "{rows}");
}

/// The json route carries the data plane, so the default is data alone: the
/// cst delegation (where ast-grep has the grammar) is not part of it.
#[test]
fn data_defaults_to_the_data_plane() {
    let rows = run(&["tests/fixtures/data/nested.json"]);
    assert!(rows.contains("\"family\":\"data\""), "{rows}");
    assert!(!rows.contains("\"family\":\"cst\""), "{rows}");
}

/// A cst-only language answers with the syntax tree, and its default stream
/// must equal `--family cst` byte for byte: nothing moved for these.
#[test]
fn a_cst_only_language_defaults_to_the_syntax_tree() {
    let default = run(&["tests/fixtures/gdscript/sample.gd"]);
    let forced = run(&["--family", "cst", "tests/fixtures/gdscript/sample.gd"]);
    assert!(default.contains("\"family\":\"cst\""), "{default}");
    assert_eq!(
        default, forced,
        "a cst-only language's default must equal --family cst"
    );
}

/// syn refuses this file, so the language's own planes answer nothing and the
/// lossless tree is the one plane left: the default uses it instead of
/// printing nothing.
#[test]
fn a_failed_native_parse_still_streams_the_syntax_tree() {
    let path = scratch("broken.rs", "definitely ( not rust");
    let arg = path.to_string_lossy().to_string();
    let rows = run(&[&arg]);
    assert!(rows.contains("\"family\":\"cst\""), "{rows}");
    assert!(!rows.contains("\"family\":\"df\""), "{rows}");
}

/// Naming a family replaces the derived default; `cst` alone is the whole
/// mask, exactly as before this change.
#[test]
fn explicit_family_still_overrides_the_default() {
    let forced = run(&["--family", "cst", "tests/fixtures/ts_checker/src/main.ts"]);
    assert!(
        forced
            .lines()
            .all(|line| line.contains("\"family\":\"cst\"")),
        "--family cst is the whole mask: {forced}"
    );
}

/// A file whose native planes parse but find nothing keeps the syntax tree:
/// a consts-only rust file has no type entities, call sites or dataflow, so
/// its type/call/df bundles come back `Some` and empty. The bundles existing
/// is not the language answering; rows are. FAIL-PRE-FIX: the predicate read
/// `Some`-ness, this file streamed zero rows, and `--bench` facts no longer
/// matched the piped line count.
#[test]
fn a_native_parse_with_no_rows_keeps_the_syntax_tree() {
    let path = scratch("consts_only.rs", "pub const K: u32 = 1;\n");
    let arg = path.to_string_lossy().to_string();
    let rows = run(&[&arg]);
    assert!(rows.contains("\"family\":\"cst\""), "rows were: {rows}");
}
