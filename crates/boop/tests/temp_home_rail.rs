//! A `boop` subprocess that inherits the real `HOME` re-parses the machine's
//! whole transcript roots (`~/.codex`, `~/.claude`, `~/.local/share/opencode`,
//! `~/.agent`) from offset zero. Every test file that spawns the binary must
//! redirect both `HOME` and `BOOP_DB` to a temp root.

use std::path::{Path, PathBuf};

/// `concatmap_e2e.rs` spawns `opencode run` through the boop binary, and that
/// child reads real model auth from the real `HOME`; the test is `#[ignore]`d
/// for exactly this reason and is exempt from the redirect.
const EXEMPT: &[&str] = &["concatmap_e2e.rs"];

/// Lane names only test code spawns. Same list as
/// `crates/boop/sql/fixture_lanes.sql`; keep the two equal.
const FIXTURE_LANES: &[&str] = &[
    "mine",
    "lane-test",
    "lane-a",
    "lane-x",
    "test-lane",
    "fake-lane",
    "some-lane",
    "orphan-lane",
    "durable-lane",
    "sibling",
    "chore-x",
];

/// Files under `src/` handing a `SpawnSpec` with no `env_stamp` to `.spawn(`.
/// The supervisor tmux starts inherits the test process `HOME`, so each run
/// appends to the machine's own `~/.agent/lanes/lane-test/supervise.log`.
const SPAWN_WAIVED: &[&str] = &[
    "harness/claude.rs",
    "harness/codex.rs",
    "harness/opencode.rs",
];

/// Files under `src/` reaching `Store::default_path()` that also name a
/// fixture lane. `supervise.rs` is the measured offender. `main.rs` matches on
/// route and tmux names its own test binary wrote 0 rows for, measured
/// 2026-08-19 by counting `agent_trace_event` around each target.
const STORE_WAIVED: &[&str] = &["main.rs", "supervise.rs"];

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

/// Every `*.rs` under `src/`, path relative to `src/`.
fn src_modules() -> Vec<(String, PathBuf)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, PathBuf)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                let name = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.push((name, path));
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    walk(&root, &root, &mut out);
    out.sort();
    out
}

/// A `#[cfg(test)]` module inside `src/` compiles into the lib test binary,
/// never names `CARGO_BIN_EXE_boop`, and reaches the machine's own `~/.agent`
/// through `dirs::home_dir()` (`trail.rs` `lanes_root`, `ident.rs`
/// `Store::default_path`). The scan above cannot see any of it.
///
/// Measured 2026-08-19 on 2672085, counting around each cargo target:
/// `cargo test -p boop --lib` alone wrote 37 `agent_trace_event` rows for lane
/// `mine` into `~/.agent/boop.db` and 663 bytes into
/// `~/.agent/lanes/lane-test/supervise.log`. Every `tests/*.rs` target wrote 0.
///
/// Two shapes reach the real root and this ratchets both:
/// a `SpawnSpec` spawned with no `env_stamp`, and a module that opens the
/// default store while naming a fixture lane. A waived file is a known
/// offender or a measured false positive, never room for one more.
#[test]
fn no_new_src_unit_test_reaches_the_machine_s_own_agent_root() {
    let mut spawners = Vec::new();
    let mut storers = Vec::new();
    let mut unused_waivers: Vec<&str> = SPAWN_WAIVED
        .iter()
        .chain(STORE_WAIVED.iter())
        .copied()
        .collect();

    for (name, path) in src_modules() {
        let text = std::fs::read_to_string(&path).unwrap();
        let spawns = text.contains("env_stamp: None") && text.contains(".spawn(&");
        let names_a_fixture = FIXTURE_LANES
            .iter()
            .any(|lane| text.contains(&format!("\"{lane}\"")));
        let stores = text.contains("Store::default_path()") && names_a_fixture;

        if spawns && !SPAWN_WAIVED.contains(&name.as_str()) {
            spawners.push(name.clone());
        }
        if stores && !STORE_WAIVED.contains(&name.as_str()) {
            storers.push(name.clone());
        }
        if spawns || stores {
            unused_waivers.retain(|waived| *waived != name);
        }
    }

    assert!(
        spawners.is_empty(),
        "these src modules spawn a SpawnSpec with env_stamp: None, so the supervisor inherits the real HOME: {spawners:?}"
    );
    assert!(
        storers.is_empty(),
        "these src modules open the default store and name a fixture lane, so their unit tests write into ~/.agent/boop.db: {storers:?}"
    );
    assert!(
        unused_waivers.is_empty(),
        "these waivers no longer match anything and must be deleted: {unused_waivers:?}"
    );
}
