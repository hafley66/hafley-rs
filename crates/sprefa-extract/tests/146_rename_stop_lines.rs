//! `extract rename` stop diagnostics carry `file:line`. The `#[path]` two-route
//! stop these cases first pinned is planned as a union now (`147`), so the
//! two-route cases pin that no stop line, byte offset, or exit 6 remains, and
//! the TS computed-member case keeps the `file:line` form.
//!
//! @comment-ok: updated in place, the cases keep their fixtures and flip to the union contract.
//! FAIL-FIRST, against the exit-6 binary: every two-route case fails on
//! `src/bin/extract.rs:1: path attr twice reaches src/bin/home.rs at runtime`.

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

/// The two-route tree plans, so neither attr line prints as a stop.
#[test]
fn a_two_route_file_prints_no_stop_line() {
    let fixture = fixture("line", TWO_ROUTE);
    let (_code, stderr) = rename_run(&fixture, "src/bin/home.rs#Thing", "Renamed");
    for line in ["src/bin/extract.rs:1:", "tests/probe.rs:3:"] {
        assert!(!stderr.contains(line), "no stop at `{line}`:\n{stderr}");
    }
}

/// A plan prints no seat at all, so the old byte-offset form stays gone too.
#[test]
fn a_two_route_file_prints_no_byte_offset() {
    let fixture = fixture("one_based", TWO_ROUTE);
    let (_code, stderr) = rename_run(&fixture, "src/bin/home.rs#Thing", "Renamed");
    assert!(
        !stderr.contains("src/bin/extract.rs byte"),
        "no byte offset reaches the reader:\n{stderr}"
    );
}

/// No seat is about a route, so no message names a reached file.
#[test]
fn a_two_route_file_names_no_reached_file() {
    let fixture = fixture("reaches", TWO_ROUTE);
    let (_code, stderr) = rename_run(&fixture, "src/bin/home.rs#Thing", "Renamed");
    assert!(
        !stderr.contains("reaches src/bin/home.rs"),
        "no stop names the reached file:\n{stderr}"
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

/// The two-route tree plans through the union, so the run exits 0 where the
/// `Dynamic` arm's 6 used to be.
#[test]
fn the_two_route_file_exits_zero() {
    let fixture = fixture("exit", TWO_ROUTE);
    let (code, stderr) = rename_run(&fixture, "src/bin/home.rs#Thing", "Renamed");
    assert_eq!(code, Some(0), "the union plans:\n{stderr}");
}
