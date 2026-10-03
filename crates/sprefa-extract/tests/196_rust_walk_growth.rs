#![cfg(feature = "cli")]
//! The demand walk's inferred bodies, counted with hafley-observe spans: they
//! grow with the bodies a seed reaches, never with the crate's unrelated files.

use std::path::PathBuf;
use std::time::Duration;

use hafley_observe::{assert_growth_sized, CountRecorder, Growth, SpanCounts};
use sprefa_extract::lang::rust_checker::{demand_walk, WalkQuestion};
use tracing_subscriber::prelude::*;

/// A crate outside every Cargo workspace: a three-body chain from `seed`,
/// plus `unrelated` modules of their own call chains.
fn crate_with(unrelated: usize) -> (tempfile::TempDir, PathBuf, Vec<(String, PathBuf)>) {
    let dir = tempfile::TempDir::new().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"growth\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
    )
    .unwrap();
    let mut lib = String::from("pub fn seed() { step(); }\nfn step() { leaf(); }\nfn leaf() {}\n");
    let mut files = vec![("src/lib.rs".to_string(), root.join("src/lib.rs"))];
    for index in 0..unrelated {
        lib.push_str(&format!("pub mod unrelated_{index};\n"));
        let path = format!("src/unrelated_{index}.rs");
        std::fs::write(
            root.join(&path),
            format!("pub fn f() {{ g(); crate::seed(); }}\nfn g() {{ let _ = vec![{index}u32].len(); }}\n"),
        )
        .unwrap();
        files.push((path.clone(), root.join(path)));
    }
    std::fs::write(root.join("src/lib.rs"), lib).unwrap();
    (dir, root, files)
}

fn walked(unrelated: usize) -> SpanCounts {
    let (_dir, root, files) = crate_with(unrelated);
    let seeds = [("src/lib.rs".to_string(), "seed".to_string())];
    let question = WalkQuestion {
        root: &root,
        files: &files,
        seeds: &seeds,
        max_depth: None,
        timeout: None,
        budget: Duration::from_secs(120),
    };
    let (recorder, layer) = CountRecorder::new();
    let answer = tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || {
        demand_walk(&question).unwrap()
    });
    assert_eq!(answer.bodies_inferred, 3);
    recorder.counts()
}

#[test]
fn bodies_inferred_stay_constant_as_unrelated_files_grow() {
    let (small, large) = (walked(2), walked(200));
    small.assert_instances("rust_walk.body", 3);
    large.assert_instances("rust_walk.body", 3);
    assert_growth_sized(&small, &large, "rust_walk.body", 2, 200, Growth::Constant);
}
