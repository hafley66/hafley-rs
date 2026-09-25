// The `extract` bin carries `required-features = ["cli"]`, so without the
// feature cargo does not rebuild it and `CARGO_BIN_EXE_extract` resolves to
// whatever an earlier `--features cli` build left in the target directory,
// whose `build.rs` wall-clock stamp differs from this crate's.
#![cfg(feature = "cli")]

use std::path::PathBuf;
use std::process::Command;

fn extract(args: &[&str]) -> std::process::Output {
    // stderr identity below needs a clock-free stream; default is info (src/trace.rs:580).
    Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(args)
        .env("RUST_LOG", "off")
        .output()
        .expect("run extract")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sprefa-cli-identity-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn help_names_the_build_and_mode_aliases() {
    let output = extract(&["--help"]);
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains(concat!("git hash: ", env!("SPREFA_BUILD_GIT_HASH"))));
    assert!(help.contains(concat!("datetime: ", env!("SPREFA_BUILD_DATETIME"))));
    assert!(help.contains("  fast "));
    assert!(help.contains("  slow "));
    assert!(help.contains("sprefa_extract=info"));
    assert!(help.contains("HAFLEY_LOG_FORMAT"));
}

#[test]
fn fast_is_the_diet_scip_family() {
    let fixture = "tests/fixtures/ts/sample.ts";
    let alias = extract(&["fast", fixture]);
    let family = extract(&["--family", "diet_scip", fixture]);
    assert!(
        alias.status.success(),
        "{}",
        String::from_utf8_lossy(&alias.stderr)
    );
    assert_eq!(alias.stdout, family.stdout);
    assert_eq!(alias.stderr, family.stderr);
}

#[test]
fn slow_is_the_scip_family_over_a_saved_index() {
    let root = "tests/fixtures/scip_rel";
    let cache = scratch("slow-cache");
    let index = cache.join("index.scip");
    std::fs::copy(
        "tests/fixtures/scip_relationship/fixture.scip",
        &index,
    )
    .expect("saved SCIP fixture");
    let set = sprefa_extract::source_set_for_root(std::path::Path::new(root)).expect("source set");
    sprefa_extract::record_index_set(&index, &set);
    let cache = cache.to_string_lossy();
    let alias = extract(&["slow", "--scip-cache", &cache, root]);
    let family = extract(&["--family", "scip", "--scip-cache", &cache, root]);
    assert!(
        alias.status.success(),
        "{}",
        String::from_utf8_lossy(&alias.stderr)
    );
    assert!(
        String::from_utf8_lossy(&alias.stdout).contains("\"record\":\"scip_def\""),
        "slow must decode the saved index"
    );
    assert_eq!(alias.stdout, family.stdout);
    assert_eq!(alias.stderr, family.stderr);
}

#[test]
fn slow_preserves_named_indexer_dispatch_and_its_separate_cache() {
    let root = "tests/fixtures/scip_rel";
    let cache = scratch("slow-named-cache");
    let picked_cache = cache.join("indexer-typescript");
    std::fs::create_dir_all(&picked_cache).expect("picked cache directory");
    let index = picked_cache.join("index.scip");
    std::fs::copy(
        "tests/fixtures/scip_relationship/fixture.scip",
        &index,
    )
    .expect("saved picked SCIP fixture");
    let set = sprefa_extract::source_set_for_root(std::path::Path::new(root)).expect("source set");
    sprefa_extract::record_index_set(&index, &set);
    let cache = cache.to_string_lossy();
    let alias = extract(&[
        "slow",
        "--indexer",
        "typescript",
        "--scip-cache",
        &cache,
        root,
    ]);
    let family = extract(&[
        "--family",
        "scip",
        "--indexer",
        "typescript",
        "--scip-cache",
        &cache,
        root,
    ]);
    assert!(
        alias.status.success(),
        "{}",
        String::from_utf8_lossy(&alias.stderr)
    );
    assert_eq!(alias.stdout, family.stdout);
    assert_eq!(alias.stderr, family.stderr);
}
