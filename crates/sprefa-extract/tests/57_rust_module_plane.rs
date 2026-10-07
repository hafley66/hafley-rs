//! The rust module plane: the Rust Reference's own `use`/`mod` resolution,
//! run once per file set, so an imported name binds the way the compiler
//! binds it and name-matching across files is only what a FREE name falls
//! to. Mirrors `54_ts_module_plane.rs`.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): before the plane,
//! `IndexBag.rust_modules` did not exist and `Resolve<CallF>`/`Resolve<TypeF>`
//! had no import leg, so `crate_path_caller` in the fixture below resolved
//! `crate_path_fn()` (unqualified) against the whole corpus name-match,
//! `super_caller`'s `root_target()` likewise, and neither `resolved_import`
//! row nor the `ambiguous`/`conflict` drop for the glob collision existed.
//!
//! Fixtures: `tests/fixtures/rust_findings/module_plane/`. The semantic
//! claims are rows in `tests/fixtures/rust_module_plane/0_plane.json`; the
//! whole edge/import/unresolved tables are one snapshot.
//!
//! A resolver that re-walked a barrel's star list per call site instead of
//! per binding would show up here as a quadratic, not as a wrong answer.
#![cfg(feature = "cli")]

use std::path::Path;
use std::process::Command;
use std::time::Instant;

#[test]
fn whole_output() {
    crate::fixture_runner::run("rust_module_plane", crate::rust_module_plane_support::evaluate);
}

// ── the plane's cost ────────────────────────────────────────────────────────

const RATIO_BUDGET: f64 = 2.5;

/// A barrel corpus of `n` leaf modules, one barrel re-exporting all of them
/// via `pub use ..::*;`, and `n` consumers each importing one name through
/// it: the shape that makes the plane work hardest, one star walk per name.
fn barrel_corpus(dir: &Path, n: usize) -> Vec<String> {
    let dir = dir.join(format!("n{n}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let mut paths = Vec::new();
    let mut barrel = String::new();
    for index in 0..n {
        let leaf = dir.join(format!("leaf{index}.rs"));
        std::fs::write(
            &leaf,
            format!("pub fn pick{index}(n: u32) -> u32 {{ n + {index} }}\n"),
        )
        .expect("leaf file");
        paths.push(leaf.to_string_lossy().into_owned());
        barrel.push_str(&format!("pub use crate::leaf{index}::*;\n"));
    }
    let barrel_path = dir.join("barrel.rs");
    std::fs::write(&barrel_path, barrel).expect("barrel file");
    paths.push(barrel_path.to_string_lossy().into_owned());
    for index in 0..n {
        let consumer = dir.join(format!("use{index}.rs"));
        std::fs::write(
            &consumer,
            format!(
                "use crate::barrel::pick{index};\n\npub fn call{index}() -> u32 {{ pick{index}({index}) }}\n"
            ),
        )
        .expect("consumer file");
        paths.push(consumer.to_string_lossy().into_owned());
    }
    paths
}

fn resolve_wall(args: &[String]) -> f64 {
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .arg("--resolve")
        .arg("--arms")
        .arg("call")
        .args(args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    start.elapsed().as_secs_f64()
}

#[test]
#[ignore = "explicit wall bench: bench/scripts/6_wall_contracts.py"]
fn barrel_resolve_wall_grows_linearly_with_file_count() {
    let dir = std::env::temp_dir().join("sprefa-extract-57-rust-module-plane");
    std::fs::create_dir_all(&dir).expect("scratch root");
    let small = barrel_corpus(&dir, 200);
    let large = barrel_corpus(&dir, 400);
    let wall200 = resolve_wall(&small);
    let wall400 = resolve_wall(&large);
    crate::wall_bench::check("tests/57_rust_module_plane.rs:barrel_resolve_wall_grows_linearly_with_file_count", (wall400 / wall200) as f64, (RATIO_BUDGET) as f64, false);
}
