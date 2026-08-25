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
/// `crates/boop-store/sql/fixture_lanes.sql`; keep the two equal.
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
/// fixture lane. `supervise.rs` is the measured offender. `cli/db.rs` and
/// `cli/job.rs` match on route and tmux names their own test binary wrote 0
/// rows for, measured 2026-08-19 by counting `agent_trace_event` around each
/// target.
const STORE_WAIVED: &[&str] = &["cli/db.rs", "cli/job.rs"];

#[test]
fn every_boop_subprocess_site_redirects_home_and_boop_db() {
    let this_file = Path::new(file!())
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();

    let mut offenders = Vec::new();
    for tests_dir in boop_crate_dirs("tests") {
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
    }

    assert!(
        offenders.is_empty(),
        "these test files spawn the boop binary without redirecting both HOME and BOOP_DB: {offenders:?}"
    );
}

/// Each `boop*` crate's `<sub>` directory that exists, so the split into
/// `boop-store` / `boop-acp` / `boop-harness` / `boop-proc` cannot drop a file
/// out of this rail's reach.
fn boop_crate_dirs(sub: &str) -> Vec<PathBuf> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut out = Vec::new();
    for entry in std::fs::read_dir(crates).unwrap() {
        let path = entry.unwrap().path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("boop") {
            continue;
        }
        let dir = path.join(sub);
        if dir.is_dir() {
            out.push(dir);
        }
    }
    out.sort();
    out
}

/// Every `*.rs` under a boop crate's `src/`, path relative to that `src/`.
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
    let mut out = Vec::new();
    for root in boop_crate_dirs("src") {
        walk(&root, &root, &mut out);
    }
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
        // A module whose test helper pins `BOOP_DB` (a `set_var` under a
        // `Once`, as `supervise.rs` does in `tempdir()`) reaches a temp store.
        let pins_store = text.contains("set_var(\"BOOP_DB\"");
        let stores = text.contains("Store::default_path()") && names_a_fixture && !pins_store;

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
