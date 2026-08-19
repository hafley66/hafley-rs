//! A `boop` subprocess that inherits the real `HOME` re-parses the machine's
//! whole transcript roots (`~/.codex`, `~/.claude`, `~/.local/share/opencode`,
//! `~/.agent`) from offset zero. Every test file that spawns the binary must
//! redirect both `HOME` and `BOOP_DB` to a temp root.

use std::path::Path;

/// `concatmap_e2e.rs` spawns `opencode run` through the boop binary, and that
/// child reads real model auth from the real `HOME`; the test is `#[ignore]`d
/// for exactly this reason and is exempt from the redirect.
const EXEMPT: &[&str] = &["concatmap_e2e.rs"];

#[test]
fn every_boop_subprocess_site_redirects_home_and_boop_db() {
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let this_file = Path::new(file!())
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&tests_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        if name == this_file || EXEMPT.contains(&name.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        if !text.contains("CARGO_BIN_EXE_boop") {
            continue;
        }
        if !text.contains(".env(\"HOME\"") || !text.contains(".env(\"BOOP_DB\"") {
            offenders.push(name);
        }
    }

    assert!(
        offenders.is_empty(),
        "these test files spawn the boop binary without redirecting both HOME and BOOP_DB: {offenders:?}"
    );
}
