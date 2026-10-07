//! The explicit one-file item-walk performance probe.
#![cfg(feature = "rust-checker")]

use std::process::Command;
use sprefa_extract::FlatFact;

const WALK_CAP_MS: u64 = 10_000;
const ITEM_ROW_CAP: usize = 1_000;

fn field(line: &str, name: &str) -> Option<u64> {
    let tail = line.split(&format!("{name}=")).nth(1)?;
    let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// One `--rust-checker` run over this crate's own `src/trace.rs`, with the
/// tier's logged wall clock and the number of rows the item walk produced.
struct Probe {
    walk_ms: u64,
    files: u64,
    rows: usize,
}

fn probe(witness: bool) -> Probe {
    let mut args = vec![
        "--resolve",
        "--arms",
        "type",
        "--root",
        ".",
        "--rust-checker",
    ];
    if witness {
        args.insert(0, "--witness");
    }
    args.push("src/trace.rs");
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        // A bare `info` turns on rust-analyzer's own span close events, whose
        // formatting cost lands inside the very phase this reads.
        .env("RUST_LOG", "sprefa_extract=info,hafley_scm=info")
        .args(&args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let loaded = stderr
        .lines()
        .find(|line| line.contains("rust checker tier loaded") && line.contains("walk_ms="))
        .unwrap_or_else(|| panic!("the tier logs its own wall clock:\n{stderr}"))
        .to_string();
    let rows = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter(|line| {
            matches!(
                serde_json::from_str::<FlatFact>(line),
                Ok(FlatFact::Fact(_))
            )
        })
        .count();
    Probe {
        walk_ms: field(&loaded, "walk_ms").expect("walk_ms is a number"),
        files: field(&loaded, "files").expect("files is a number"),
        rows,
    }
}

/// The 10-second law over `src/trace.rs`. `walk_ms` also covers the per-file
/// resolve leg, so the item walk's own price is what the envelope adds.
#[test]
#[ignore = "loads a rust-analyzer workspace over this crate twice; run with --ignored"]
fn walk_time_is_priced_by_the_file() {
    let described = probe(true);
    let resolve_only = probe(false);
    assert_eq!(described.files, 1);
    assert_eq!(resolve_only.rows, 0, "no envelope, no item walk");

    let item_walk = described.walk_ms.saturating_sub(resolve_only.walk_ms);
    crate::wall_bench::check("tests/108_rust_checker_walk_by_file.rs:walk_time_is_priced_by_the_file", (item_walk) as f64, (WALK_CAP_MS) as f64, false);
    assert!(
        described.rows < ITEM_ROW_CAP,
        "{} rows for one 254-line file",
        described.rows
    );
}
