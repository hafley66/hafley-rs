//! `ryi rename` on a TS anchor plans every site the scope plane binds and lists
//! every site it declines: exit 7, a one-seat plan, a one-row abstain list. The
//! arm types no receiver (`issues/ts-field-rename`), so the abstained site is a
//! member access on an untraced receiver and the planned seat is the
//! declaration itself.
//! @comment-ok: fail-first receipt, repo law keeps these in TEST headers.
//! FAIL-FIRST, against the pre-abstain binary: exit 6 with the `member access`
//! seat on stderr and no plan on stdout.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ANCHOR: &str = "src/app.ts";

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn fixture() -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_rename_abstain_ts_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts_rename_abstain/before");
    let target = root.join("src");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::copy(source.join(ANCHOR), root.join(ANCHOR)).expect("copy fixture file");
    Fixture {
        root: root.canonicalize().unwrap(),
        state,
    }
}

fn rename_verb(fixture: &Fixture, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ryi"))
        .arg("rename")
        .arg(format!("{ANCHOR}#old"))
        .arg("fresh")
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(extra)
        .output()
        .expect("extract binary runs")
}

/// stdout minus the lines that carry the temp root or the stage id.
fn report(output: &Output) -> String {
    String::from_utf8(output.stdout.clone())
        .expect("stdout is UTF-8")
        .lines()
        .filter(|line| {
            line.starts_with("plan ")
                || line.starts_with("  src/")
                || line.contains(": abstain ")
                || line.starts_with("{\"abstains\"")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The declaration is the one planned seat; `probe.old()` is the one abstain.
/// The run exits 7 and leaves the tree alone.
#[test]
fn untyped_receiver_abstains_beside_a_one_seat_plan() {
    let fixture = fixture();
    let output = rename_verb(&fixture, &[]);
    assert_eq!(
        output.status.code(),
        Some(7),
        "plan plus abstains exits 7:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        report(&output),
        "plan src/app.ts old -> fresh\n  src/app.ts  1 uses\nsrc/app.ts:4: abstain inferred receiver=probe"
    );
    assert!(String::from_utf8_lossy(&output.stderr).is_empty());
    let tree = std::fs::read_to_string(fixture.root.join(ANCHOR)).unwrap();
    assert!(
        tree.contains("export function old()"),
        "dry run edits nothing"
    );
}

/// `--json` closes stdout with the `abstains` array and drops the text lines.
#[test]
fn json_carries_the_abstain_list() {
    let fixture = fixture();
    let output = rename_verb(&fixture, &["--json"]);
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(
        report(&output),
        "plan src/app.ts old -> fresh\n  src/app.ts  1 uses\n{\"abstains\":[{\"file\":\"src/app.ts\",\"line\":4,\"span\":{\"start\":54,\"len\":3},\"symbol\":\"old\",\"reason\":\"inferred\",\"receiver\":\"probe\"}]}"
    );
}
