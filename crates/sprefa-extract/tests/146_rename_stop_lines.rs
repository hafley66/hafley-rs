//! `extract rename` stop diagnostics carry `file:line`, and a `#[path]` stop
//! names the file its route reaches. The old form printed a bare byte offset,
//! which the reader had to resolve by hand, and the route stayed invisible.
//!
//! @comment-ok: fail-first receipt, repo law keeps these in TEST headers.
//! FAIL-FIRST, against the byte-offset binary: every two-route case fails on
//! `src/bin/extract.rs byte N` where `src/bin/extract.rs:1` is expected.

use std::path::PathBuf;
use std::process::Command;

/// `(rel, text)` pairs laid under the fixture root before the run. The two
/// routes are crate-root owners: a bin root and a test root whose `#[path]`
/// values join to the same reached file, the crate's own double reach.
const TWO_ROUTE: &[(&str, &str)] = &[
    ("src/lib.rs", "pub fn root() {}\n"),
    ("src/bin/extract.rs", "#[path = \"home.rs\"] mod home;\n"),
    (
        "tests/probe.rs",
        "// a leading comment moves the attr off line one\n\n#[path = \"../src/bin/home.rs\"] mod home;\n",
    ),
    ("src/bin/home.rs", "pub struct Thing;\n"),
];

const TS_DYNAMIC: &[(&str, &str)] = &[(
    "src/app.ts",
    "class Widget {}\n\nconst viaComputed = { mark: 1 }[\"Widget\"];\n",
)];

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn fixture(label: &str, files: &[(&str, &str)]) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_rename_stop_lines_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).expect("create state dir");
    for (rel, text) in files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("rel has a parent")).expect("create dir");
        std::fs::write(path, text).expect("write fixture file");
    }
    Fixture {
        root: root.canonicalize().expect("canonicalize fixture root"),
        state,
    }
}

/// The rename run's exit code and stderr: the stop diagnostics live on stderr.
fn rename_run(fixture: &Fixture, target: &str, new: &str) -> (Option<i32>, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .arg("rename")
        .arg(target)
        .arg(new)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .output()
        .expect("extract binary runs");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

/// Both seats print the declaring file, the attr's one-based line, and the
/// file the route reaches: the bin root on line 1, the test root on line 3.
#[test]
fn the_stop_prints_file_and_line_not_a_byte_offset() {
    let fixture = fixture("line", TWO_ROUTE);
    let (_code, stderr) = rename_run(&fixture, "src/bin/home.rs#Thing", "Renamed");
    for line in [
        "src/bin/extract.rs:1: path attr twice reaches src/bin/home.rs at runtime",
        "tests/probe.rs:3: path attr twice reaches src/bin/home.rs at runtime",
    ] {
        assert!(stderr.contains(line), "expected `{line}`:\n{stderr}");
    }
}

/// A line number is one-based: an attr on the first line prints `:1`, so a
/// `:0` would mean the table fed the formatter a zero-based row.
#[test]
fn an_attr_on_the_first_line_prints_line_one() {
    let fixture = fixture("one_based", TWO_ROUTE);
    let (_code, stderr) = rename_run(&fixture, "src/bin/home.rs#Thing", "Renamed");
    assert!(
        stderr.contains("src/bin/extract.rs:1:"),
        "the first-line attr prints line one:\n{stderr}"
    );
    assert!(
        !stderr.contains("src/bin/extract.rs:0"),
        "no zero-based line reaches the reader:\n{stderr}"
    );
}

/// The seat is about a route, so the message names the reached file instead of
/// leaving the reader to re-derive where the attr's value lands.
#[test]
fn the_stop_names_the_file_the_route_reaches() {
    let fixture = fixture("reaches", TWO_ROUTE);
    let (_code, stderr) = rename_run(&fixture, "src/bin/home.rs#Thing", "Renamed");
    assert!(
        stderr.contains("reaches src/bin/home.rs at runtime"),
        "the stop names the reached file:\n{stderr}"
    );
}

/// Seats with no route keep the bare `file:line` form: the whole line is the
/// form plus `reaches the symbol at runtime`, with no reached file named.
#[test]
fn other_arms_keep_the_bare_file_and_line_form() {
    let fixture = fixture("ts", TS_DYNAMIC);
    let (_code, stderr) = rename_run(&fixture, "src/app.ts#Widget", "Gadget");
    assert!(
        stderr.contains("src/app.ts:3: computed member reaches the symbol at runtime"),
        "the computed member seat keeps the bare form:\n{stderr}"
    );
    assert!(
        !stderr.contains("reaches src/"),
        "a seat with no route names no file:\n{stderr}"
    );
}

/// The diagnostic rewrite does not move the refuse-vs-plan line: the two-route
/// tree still refuses through the `Dynamic` arm, whose exit code is 6.
#[test]
fn the_two_route_stop_still_exits_six() {
    let fixture = fixture("exit", TWO_ROUTE);
    let (code, stderr) = rename_run(&fixture, "src/bin/home.rs#Thing", "Renamed");
    assert_eq!(code, Some(6), "Dynamic exits 6:\n{stderr}");
}
