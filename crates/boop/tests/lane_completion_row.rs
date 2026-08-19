//! FAIL-PRE-FIX: one lane exit put TWO `kind=result` rows in the parent's
//! mailbox (supervisor + pane epilogue, ids minted apart), so every completion
//! reached the coordinator inbox twice. On the pre-fix tree this counts 2.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::Duration;

use boop::channel::{Delivery, LaneChannel, TurnEvent};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("boop-completion-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(dir.join("home")).unwrap();
    dir
}

/// A harness that completes its first turn and reports nothing else.
struct DoneChannel;

impl LaneChannel for DoneChannel {
    fn conversation_id(&self) -> Option<String> {
        None
    }
    fn start_turn(&mut self, _text: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn steer(&mut self, _text: &str) -> anyhow::Result<Delivery> {
        Ok(Delivery::MidTurn)
    }
    fn next_event(&mut self, _timeout: Duration) -> anyhow::Result<Option<TurnEvent>> {
        Ok(Some(TurnEvent::ok("completed")))
    }
    fn close(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

fn result_rows(dir: &Path, lane: &str) -> Vec<String> {
    let mut rows = Vec::new();
    for path in boop::bus::read_boxes(dir).unwrap_or_default() {
        rows.extend(boop::bus::parse_box(&path));
    }
    rows.into_iter()
        .filter(|row| row.kind == "result" && row.from == lane)
        .map(|row| row.body)
        .collect()
}

/// A `boop` on PATH for the epilogue's own command line, taken from this
/// build rather than from `~/.cargo/bin`.
fn path_with_boop(dir: &Path) -> String {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let link = bin.join("boop");
    let _ = std::fs::remove_file(&link);
    symlink(env!("CARGO_BIN_EXE_boop"), &link).unwrap();
    format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

#[test]
fn one_lane_exit_writes_exactly_one_result_row() {
    let dir = tempdir("one-row");
    std::fs::write(
        dir.join("registry.json"),
        serde_json::json!({ "mine": { "kind": "lane", "parent": "coordinator" } }).to_string(),
    )
    .unwrap();
    let brief = dir.join("brief.md");
    std::fs::write(&brief, "do the work\n").unwrap();

    let rc = boop::supervise::run(
        boop::supervise::LaneRun {
            lane: "mine".to_owned(),
            brief,
            mail_dir: dir.clone(),
            cwd: dir.clone(),
            model: None,
            resume: None,
        },
        &mut DoneChannel,
    )
    .unwrap();
    assert_eq!(rc, 0);

    let epilogue = boop::lane::pane_epilogue("mine", &dir);
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("__rc=0; {epilogue}"))
        .env("PATH", path_with_boop(&dir))
        .env("HOME", dir.join("home"))
        .env("BOOP_DB", dir.join("boop.db"))
        .status()
        .unwrap();
    assert!(status.success(), "the epilogue itself must succeed");

    let rows = result_rows(&dir, "mine");
    assert_eq!(rows, vec!["lane mine done rc=0".to_owned()]);

    let routes = boop::bus::read_routes(&dir).unwrap();
    assert!(
        !routes.contains_key("mine"),
        "the epilogue still drops the lane's route"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
