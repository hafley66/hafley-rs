#![cfg(feature = "rust-checker")]

use hafley_scm::read::lang::rust_checker::module_tree;
use std::path::Path;
use std::time::{Duration, Instant};

#[test]
#[ignore = "one measurement run on the repository workspaces"]
fn cold_and_warm_names_hosts() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    println!("workspace\tstate\tload_seconds\tquery_seconds\tplaces");
    for (name, source) in [
        ("hafley-rs", repo.join("crates/hafley_scm/src/lib.rs")),
        (
            "sprefa-extract",
            repo.join("crates/sprefa-extract/src/lib.rs"),
        ),
    ] {
        for state in ["cold", "warm"] {
            let started = Instant::now();
            let tree = module_tree(&source, Duration::from_secs(120)).unwrap();
            let load = started.elapsed();
            let started = Instant::now();
            let places = tree.places(&source);
            println!(
                "{name}\t{state}\t{:.6}\t{:.6}\t{}",
                load.as_secs_f64(),
                started.elapsed().as_secs_f64(),
                places.len()
            );
            assert!(!places.is_empty());
        }
    }
}
