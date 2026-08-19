//! Pins `boop tell-parent` (parent-edge resolution through the identity
//! ladder's env rung, the registered-coordinator fallback, the `yield`
//! default body) and `boop tell-children` (per-child hook/dead reporting).

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const BOOP: &str = env!("CARGO_BIN_EXE_boop");
/// A clock reading is not an identifier (sprefa failure ledger 54).
static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A temp root with its own mailbox, HOME and store, so the suite never
/// touches the machine's live registry.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!(
            "boop-tell-{}-{}-{tag}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("mail")).unwrap();
        std::fs::create_dir_all(root.join("home")).unwrap();
        Fixture { root }
    }

    fn mail(&self) -> PathBuf {
        self.root.join("mail")
    }

    /// Write `registry.json` from a route map, in the shape
    /// `bus::read_routes` parses (name -> route object).
    fn write_registry(&self, routes: serde_json::Value) {
        std::fs::write(
            self.mail().join("registry.json"),
            serde_json::to_string(&routes).unwrap(),
        )
        .unwrap();
    }

    /// Every row `bus.ndjson` holds, in file order. Empty when the file was
    /// never written, the way a failed send leaves it.
    fn bus_rows(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.mail().join("bus.ndjson"))
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// `boop <args>` run as `caller`, resolved through the env rung
    /// (`BOOP_SESSION`/`BOOP_LANE`) against this fixture's own mail dir, HOME
    /// and store.
    fn boop_as(&self, caller: &str, args: &[&str]) -> Output {
        Command::new(BOOP)
            .args(args)
            .arg("--mail-dir")
            .arg(self.mail())
            .env("HOME", self.root.join("home"))
            .env("BOOP_DB", self.root.join("boop.db"))
            .env("BOOP_SESSION", caller)
            .env("BOOP_LANE", caller)
            .output()
            .unwrap()
    }

    /// `boop <args>` with no caller identity, for verbs that resolve none.
    fn boop(&self, args: &[&str]) -> Output {
        Command::new(BOOP)
            .args(args)
            .env("HOME", self.root.join("home"))
            .env("BOOP_DB", self.root.join("boop.db"))
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn tell_parent_lands_exactly_one_row_of_the_given_kind_addressed_to_the_recorded_parent_edge() {
    let fixture = Fixture::new("edge");
    fixture.write_registry(serde_json::json!({
        "feature-a": {"kind": "lane", "parent": "coord-1"},
        "coord-1": {"kind": "coordinator"},
    }));
    let output = fixture.boop_as(
        "feature-a",
        &["tell-parent", "--kind", "completion", "--body", "done here"],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("feature-a -> coord-1 (parent from edge)"),
        "stdout: {text}"
    );
    let last = lines.last().copied().unwrap_or_default();

    let rows = fixture.bus_rows();
    assert_eq!(rows.len(), 1, "bus rows: {rows:?}");
    let row = &rows[0];
    assert_eq!(row["from"], "feature-a", "row: {row}");
    assert_eq!(row["to"], "coord-1", "row: {row}");
    assert_eq!(row["kind"], "completion", "row: {row}");
    assert_eq!(row["body"], "done here", "row: {row}");
    assert_eq!(
        row["id"].as_str(),
        Some(last),
        "row: {row}, last stdout line: {last}"
    );
}

#[test]
fn a_caller_with_no_recorded_parent_falls_back_to_the_one_registered_coordinator() {
    let fixture = Fixture::new("fallback");
    let absent_pane = format!("boop-tell-absent-{}", std::process::id());
    fixture.write_registry(serde_json::json!({
        "feature-b": {"kind": "lane"},
        "coord-2": {"kind": "coordinator", "tmux": absent_pane},
    }));
    let output = fixture.boop_as(
        "feature-b",
        &["tell-parent", "--kind", "note", "--body", "hi"],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("(parent from registry)"), "stdout: {text}");

    let rows = fixture.bus_rows();
    assert_eq!(rows.len(), 1, "bus rows: {rows:?}");
    assert_eq!(rows[0]["to"], "coord-2", "row: {}", rows[0]);
}

#[test]
fn a_caller_with_no_parent_edge_and_no_registered_coordinator_fails_by_name() {
    let fixture = Fixture::new("noparent");
    fixture.write_registry(serde_json::json!({
        "solo": {"kind": "lane"},
    }));
    let output = fixture.boop_as("solo", &["tell-parent", "--kind", "note", "--body", "hi"]);
    assert!(!output.status.success(), "stdout: {}", stdout(&output));
    let err = stderr(&output);
    assert!(err.contains("no parent edge:"), "stderr: {err}");
    assert!(err.contains("solo"), "stderr: {err}");
    let rows = fixture.bus_rows();
    assert!(rows.is_empty(), "rows: {rows:?}");
}

#[test]
fn kind_yield_with_no_body_mints_the_default_body() {
    let fixture = Fixture::new("yield");
    fixture.write_registry(serde_json::json!({
        "feature-a": {"kind": "lane", "parent": "coord-1"},
        "coord-1": {"kind": "coordinator"},
    }));
    let output = fixture.boop_as("feature-a", &["tell-parent", "--kind", "yield"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let rows = fixture.bus_rows();
    assert_eq!(rows.len(), 1, "bus rows: {rows:?}");
    assert_eq!(rows[0]["kind"], "yield", "row: {}", rows[0]);
    assert_eq!(
        rows[0]["body"],
        "yield feature-a rc=0 branch=- head=-",
        "row: {}",
        rows[0]
    );
}

#[test]
fn kind_completion_with_no_body_is_an_error_naming_the_flag() {
    let fixture = Fixture::new("nobody");
    fixture.write_registry(serde_json::json!({
        "feature-a": {"kind": "lane", "parent": "coord-1"},
        "coord-1": {"kind": "coordinator"},
    }));
    let output = fixture.boop_as("feature-a", &["tell-parent", "--kind", "completion"]);
    assert!(!output.status.success(), "stdout: {}", stdout(&output));
    let err = stderr(&output);
    assert!(err.contains("--body is required"), "stderr: {err}");
}

#[test]
fn tell_children_lands_on_the_hook_child_and_reports_the_pane_less_dead_child_dead() {
    let fixture = Fixture::new("children");
    let hook_child_cwd = fixture.root.join("hook-child-project");
    std::fs::create_dir_all(hook_child_cwd.join(".claude")).unwrap();
    std::fs::write(
        hook_child_cwd.join(".claude").join("settings.json"),
        serde_json::to_string(&serde_json::json!({
            "hooks": {
                "Stop": [{
                    "hooks": [{
                        "type": "command",
                        "command": "boop inbox drain --as hook-child --hook stop",
                        "timeout": 30,
                    }],
                }],
            },
        }))
        .unwrap(),
    )
    .unwrap();
    fixture.write_registry(serde_json::json!({
        "coord-3": {"kind": "coordinator"},
        // Pane-less: no cwd hook and no tmux, the way `child_reach` reports dead.
        "dead-child": {"kind": "lane", "parent": "coord-3"},
        "hook-child": {
            "kind": "lane",
            "parent": "coord-3",
            "cwd": hook_child_cwd.to_str().unwrap(),
        },
    }));
    let output = fixture.boop_as("coord-3", &["tell-children", "--body", "status check"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "stdout: {text}");
    // BTreeMap order: "dead-child" sorts before "hook-child".
    assert_eq!(lines[0], "dead dead-child", "stdout: {text}");
    assert!(
        lines[1].starts_with("landed hook-child ") && lines[1].ends_with("(hook inbox)"),
        "stdout: {text}"
    );

    let rows = fixture.bus_rows();
    assert_eq!(rows.len(), 1, "bus rows: {rows:?}");
    let row = &rows[0];
    assert_eq!(row["from"], "coord-3", "row: {row}");
    assert_eq!(row["to"], "hook-child", "row: {row}");
    assert_eq!(row["kind"], "note", "row: {row}");
    assert_eq!(row["body"], "status check", "row: {row}");
    let message_id = lines[1]
        .split_whitespace()
        .find(|word| word.starts_with("m-"))
        .expect("the landed line names the message id");
    assert_eq!(row["id"].as_str(), Some(message_id), "row: {row}");
}

#[test]
fn tell_children_with_no_children_at_all_says_so_and_exits_clean() {
    let fixture = Fixture::new("nochildren");
    fixture.write_registry(serde_json::json!({
        "coord-4": {"kind": "coordinator"},
    }));
    let output = fixture.boop_as("coord-4", &["tell-children", "--body", "anyone there"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(
        stdout(&output).trim(),
        "no child of coord-4 is registered",
        "stdout: {}",
        stdout(&output)
    );
    assert!(fixture.bus_rows().is_empty(), "rows: {:?}", fixture.bus_rows());
}

#[test]
fn boop_help_doctrine_names_both_verbs() {
    let fixture = Fixture::new("help");
    let output = fixture.boop(&["--help"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("boop tell-parent"), "stdout: {text}");
    assert!(text.contains("boop tell-children"), "stdout: {text}");
}
