//! Pins `boop tell-parent` (parent-edge resolution through the identity
//! ladder's env rung, the registered-coordinator fallback, the `yield`
//! default body) and `boop tell-children` (per-child landed/no-route/dead).

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

    /// Every row the mailbox holds, in append order. Empty when the send
    /// never wrote one, the way a failed send leaves it.
    fn bus_rows(&self) -> Vec<serde_json::Value> {
        boop_store::testing::mail_rows(&self.mail().join("boop.db"))
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
        rows[0]["body"], "yield feature-a rc=0 branch=- head=-",
        "row: {}",
        rows[0]
    );
}

#[test]
fn kind_completion_with_no_body_is_an_error_naming_the_kind() {
    let fixture = Fixture::new("nobody");
    fixture.write_registry(serde_json::json!({
        "feature-a": {"kind": "lane", "parent": "coord-1"},
        "coord-1": {"kind": "coordinator"},
    }));
    let output = fixture.boop_as("feature-a", &["tell-parent", "--kind", "completion"]);
    assert!(!output.status.success(), "stdout: {}", stdout(&output));
    let err = stderr(&output);
    assert!(err.contains("a body is required"), "stderr: {err}");
    assert!(err.contains("--kind completion"), "stderr: {err}");
}

#[test]
fn tell_children_lands_on_the_hook_child_and_reports_the_routeless_child_as_no_route() {
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
        // Pane-less: no cwd hook and no tmux, so nothing ever addressed it.
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
    assert_eq!(lines.len(), 3, "stdout: {text}");
    // BTreeMap order: "dead-child" sorts before "hook-child".
    assert_eq!(
        lines[0], "no-route dead-child (no hook, no pane)",
        "stdout: {text}"
    );
    assert!(
        lines[1].starts_with("landed hook-child ") && lines[1].ends_with("(hook inbox)"),
        "stdout: {text}"
    );
    assert_eq!(lines[2], "1 landed, 1 no-route, 0 dead", "stdout: {text}");

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
    assert!(
        fixture.bus_rows().is_empty(),
        "rows: {:?}",
        fixture.bus_rows()
    );
}

/// A claude Agent-tool child owns no pane and no route, so the row has nowhere
/// to land. The verb says so per target instead of exiting clean on an empty
/// registry child list.
#[test]
fn tell_children_names_a_native_subagent_child_as_no_route() {
    let fixture = Fixture::new("native");
    fixture.write_registry(serde_json::json!({
        "coord-6": {"kind": "coordinator", "sessionId": "coord-6"},
    }));
    let store = boop::Store::open(fixture.root.join("boop.db")).unwrap();
    store
        .add_edge_at("coord-6", "coord-6/agent-a1b2", "spawned", 7)
        .unwrap();
    drop(store);

    let output = fixture.boop_as("coord-6", &["tell-children", "--body", "ping"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "stdout: {text}");
    assert!(
        lines[0].starts_with("no-route coord-6/agent-a1b2 (native subagent"),
        "stdout: {text}"
    );
    assert_eq!(lines[1], "0 landed, 1 no-route, 0 dead", "stdout: {text}");
    assert!(
        fixture.bus_rows().is_empty(),
        "rows: {:?}",
        fixture.bus_rows()
    );
}

#[test]
fn boop_help_doctrine_names_the_one_send_and_the_wait() {
    let fixture = Fixture::new("help");
    let output = fixture.boop(&["--help"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("boop beep <route> <body>"), "stdout: {text}");
    assert!(text.contains("boop beep parent"), "stdout: {text}");
    assert!(text.contains("boop beep children"), "stdout: {text}");
    assert!(text.contains("boop wait --me"), "stdout: {text}");
    // The folded spellings still run; the doctrine no longer teaches them.
    for folded in ["boop push <route>", "boop tell-parent [", "boop tell-children -"] {
        assert!(!text.contains(folded), "doctrine still teaches {folded}:\n{text}");
    }
}

/// Defect 2 (addendum 2026-08-25): a native subagent runs inside its
/// spawner's environment, so the env rung names the spawner. `--as` outranks
/// it and the row leaves the native's own parent edge.
#[test]
fn tell_parent_as_a_native_outranks_the_spawners_env_stamp() {
    let fixture = Fixture::new("as-native");
    fixture.write_registry(serde_json::json!({
        "feature-a": {"kind": "lane", "parent": "coord-1"},
        "native-n1": {"kind": "native", "parent": "feature-a"},
        "coord-1": {"kind": "coordinator"},
    }));
    let output = fixture.boop_as(
        "feature-a",
        &[
            "tell-parent",
            "--as",
            "native-n1",
            "--kind",
            "note",
            "--body",
            "from the native",
        ],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let rows = fixture.bus_rows();
    assert_eq!(rows.len(), 1, "bus rows: {rows:?}");
    assert_eq!(rows[0]["from"], "native-n1");
    assert_eq!(rows[0]["to"], "feature-a");
}

/// `--as` naming nothing in the registry is an error with the fix in it, not
/// a silent fall back to the env rung.
#[test]
fn tell_parent_as_an_unregistered_name_says_to_register_it() {
    let fixture = Fixture::new("as-unknown");
    fixture.write_registry(serde_json::json!({
        "feature-a": {"kind": "lane", "parent": "coord-1"},
        "coord-1": {"kind": "coordinator"},
    }));
    let output = fixture.boop_as(
        "feature-a",
        &["tell-parent", "--as", "ghost", "--body", "hello"],
    );
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("beep agent register ghost"),
        "stderr: {}",
        stderr(&output)
    );
}

/// Defect 2: the registration's last line is the export the native evals, so
/// every later `--me` verb watches the native's own inbox.
#[test]
fn agent_register_ends_with_the_export_line_for_the_new_name() {
    let fixture = Fixture::new("register-export");
    fixture.write_registry(serde_json::json!({
        "coord-1": {"kind": "coordinator"},
    }));
    let output = fixture.boop(&[
        "beep",
        "agent",
        "register",
        "native-n1",
        "--parent",
        "coord-1",
        "--mail-dir",
        fixture.mail().to_str().unwrap(),
    ]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert_eq!(
        text.lines().last(),
        Some("export BOOP_SESSION=native-n1 BOOP_LANE=native-n1"),
        "stdout: {text}"
    );
}

/// Defect 2: `push` and `beep hail` take the same `--as` spelling `wait --me`
/// takes, so one brief line serves every verb a native runs.
#[test]
fn push_and_hail_accept_the_as_spelling_for_the_sender() {
    let fixture = Fixture::new("as-spelling");
    fixture.write_registry(serde_json::json!({
        "native-n1": {"kind": "native", "parent": "feature-a"},
        "native-n2": {"kind": "native", "parent": "feature-b"},
    }));
    let output = fixture.boop_as(
        "feature-b",
        &[
            "beep",
            "hail",
            "native-n1",
            "--as",
            "native-n2",
            "--body",
            "ping",
        ],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let rows = fixture.bus_rows();
    assert_eq!(rows.len(), 1, "bus rows: {rows:?}");
    assert_eq!(rows[0]["from"], "native-n2");
    assert_eq!(rows[0]["to"], "native-n1");
}

/// Defect 4 (addendum 2026-08-25): `push parent` and `beep hail parent`
/// advertise the alias in their help, then addressed a route literally named
/// `parent` and left the row held with no registry route.
#[test]
fn hail_parent_resolves_the_alias_through_the_same_edge_tell_parent_walks() {
    let fixture = Fixture::new("hail-parent");
    fixture.write_registry(serde_json::json!({
        "native-n1": {"kind": "native", "parent": "feature-cx-a"},
        "feature-cx-a": {"kind": "lane", "parent": "coord-1"},
        "coord-1": {"kind": "coordinator"},
    }));
    let output = fixture.boop_as(
        "feature-cx-a",
        &[
            "beep", "hail", "parent", "--as", "native-n1", "--body", "up one",
        ],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let rows = fixture.bus_rows();
    assert_eq!(rows.len(), 1, "bus rows: {rows:?}");
    assert_eq!(rows[0]["to"], "feature-cx-a", "the alias resolved");
    assert_eq!(rows[0]["from"], "native-n1");
}

#[test]
fn push_parent_resolves_the_alias_before_it_walks_the_ladder() {
    let fixture = Fixture::new("push-parent");
    fixture.write_registry(serde_json::json!({
        "native-n1": {"kind": "native", "parent": "feature-cx-a"},
        "feature-cx-a": {"kind": "lane", "parent": "coord-1"},
        "coord-1": {"kind": "coordinator"},
    }));
    let output = fixture.boop_as(
        "feature-cx-a",
        &[
            "push", "parent", "--as", "native-n1", "--body", "up one", "--timeout", "1",
        ],
    );
    // No parent answers inside a second, so the block times out. The row it
    // appended first is what this pins.
    assert_eq!(output.status.code(), Some(124), "stderr: {}", stderr(&output));
    let rows = fixture.bus_rows();
    assert_eq!(rows.len(), 1, "bus rows: {rows:?}");
    assert_eq!(rows[0]["to"], "feature-cx-a", "the alias resolved");
    assert_eq!(rows[0]["from"], "native-n1");
}

/// A caller whose parent edge cannot be resolved gets that error, never a row
/// addressed to the literal word.
#[test]
fn push_parent_with_no_edge_fails_by_name_instead_of_addressing_the_word() {
    let fixture = Fixture::new("push-parent-noedge");
    fixture.write_registry(serde_json::json!({
        "lonely": {"kind": "coordinator"},
    }));
    let output = fixture.boop_as(
        "lonely",
        &["push", "parent", "--body", "up one", "--timeout", "1"],
    );
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("no parent edge"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(fixture.bus_rows().is_empty(), "no row was appended");
}

// ---------------------------------------------------------------------------
// the fold: one send, four hidden aliases
// ---------------------------------------------------------------------------

/// The registry every alias test sends against: a native under a lane under a
/// coordinator, so `parent` and `children` both resolve.
fn folded_registry() -> serde_json::Value {
    serde_json::json!({
        "coord-1": {"kind": "coordinator"},
        "feature-a": {"kind": "lane", "parent": "coord-1"},
        "native-n1": {"kind": "native", "parent": "feature-a"},
    })
}

/// The row one send left, as `(from, to, kind, body)`.
fn only_row(fixture: &Fixture) -> (String, String, String, String) {
    let rows = fixture.bus_rows();
    assert_eq!(rows.len(), 1, "bus rows: {rows:?}");
    let row = &rows[0];
    (
        row["from"].as_str().unwrap().to_owned(),
        row["to"].as_str().unwrap().to_owned(),
        row["kind"].as_str().unwrap().to_owned(),
        row["body"].as_str().unwrap().to_owned(),
    )
}

/// `boop beep <route> <body>`: the one send, in the spelling the doctrine now
/// teaches. Every case below has to leave the same row this one leaves.
#[test]
fn beep_sends_one_row_from_a_route_and_a_body_positional() {
    let fixture = Fixture::new("beep-send");
    fixture.write_registry(folded_registry());
    let output = fixture.boop_as(
        "feature-a",
        &["beep", "native-n1", "the body", "--kind", "note", "--no-wait"],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(
        only_row(&fixture),
        (
            "feature-a".to_owned(),
            "native-n1".to_owned(),
            "note".to_owned(),
            "the body".to_owned()
        )
    );
}

/// `--body` is the older spelling of the positional, kept so briefs written
/// against `push` and `beep hail` keep running.
#[test]
fn beep_still_takes_the_body_flag_spelling() {
    let fixture = Fixture::new("beep-body-flag");
    fixture.write_registry(folded_registry());
    let output = fixture.boop_as(
        "feature-a",
        &[
            "beep",
            "native-n1",
            "--body",
            "the body",
            "--kind",
            "note",
            "--no-wait",
        ],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(only_row(&fixture).3, "the body");
}

/// Alias 1: `boop beep hail`. Same row, same rung.
#[test]
fn beep_hail_is_an_alias_of_the_one_send() {
    let fixture = Fixture::new("alias-hail");
    fixture.write_registry(folded_registry());
    let output = fixture.boop_as(
        "feature-a",
        &[
            "beep",
            "hail",
            "native-n1",
            "--body",
            "the body",
            "--kind",
            "note",
        ],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(
        only_row(&fixture),
        (
            "feature-a".to_owned(),
            "native-n1".to_owned(),
            "note".to_owned(),
            "the body".to_owned()
        )
    );
}

/// Alias 2: `boop push`. Same row; it blocks after, so the timeout is short.
#[test]
fn push_is_an_alias_of_the_one_send() {
    let fixture = Fixture::new("alias-push");
    fixture.write_registry(folded_registry());
    let output = fixture.boop_as(
        "feature-a",
        &[
            "push",
            "native-n1",
            "--body",
            "the body",
            "--kind",
            "note",
            "--timeout",
            "1",
        ],
    );
    assert_eq!(output.status.code(), Some(124), "stderr: {}", stderr(&output));
    assert_eq!(
        only_row(&fixture),
        (
            "feature-a".to_owned(),
            "native-n1".to_owned(),
            "note".to_owned(),
            "the body".to_owned()
        )
    );
}

/// Alias 3: `boop tell-parent` == `boop beep parent <body>`.
#[test]
fn tell_parent_is_an_alias_of_beep_parent() {
    let alias = Fixture::new("alias-parent-old");
    alias.write_registry(folded_registry());
    let output = alias.boop_as(
        "native-n1",
        &["tell-parent", "--kind", "note", "--body", "the body"],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let folded = Fixture::new("alias-parent-new");
    folded.write_registry(folded_registry());
    let output = folded.boop_as(
        "native-n1",
        &["beep", "parent", "the body", "--kind", "note", "--no-wait"],
    );
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    assert_eq!(only_row(&alias).0, only_row(&folded).0);
    assert_eq!(only_row(&alias).1, "feature-a");
    assert_eq!(only_row(&alias).1, only_row(&folded).1);
    assert_eq!(only_row(&alias).3, only_row(&folded).3);
}

/// Alias 4: `boop tell-children` == `boop beep children <body>`. The fan-out
/// reports per target, so the tally line is what the two have to share.
#[test]
fn tell_children_is_an_alias_of_beep_children() {
    let alias = Fixture::new("alias-children-old");
    alias.write_registry(folded_registry());
    let old = alias.boop_as("feature-a", &["tell-children", "--body", "stop"]);
    assert!(old.status.success(), "stderr: {}", stderr(&old));

    let folded = Fixture::new("alias-children-new");
    folded.write_registry(folded_registry());
    let new = folded.boop_as("feature-a", &["beep", "children", "stop"]);
    assert!(new.status.success(), "stderr: {}", stderr(&new));

    assert_eq!(stdout(&old), stdout(&new));
    assert!(stdout(&old).contains("no-route native-n1"), "{}", stdout(&old));
}

/// A route named after a `beep` subcommand can never be addressed, because
/// clap resolves the name to that subcommand first. The send says so.
#[test]
fn a_route_named_after_a_beep_subcommand_is_refused_by_name() {
    let fixture = Fixture::new("reserved-route");
    fixture.write_registry(folded_registry());
    let output = fixture.boop_as("feature-a", &["push", "lane", "--body", "x"]);
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("`lane` is the name of a `boop beep` subcommand"), "stderr: {err}");
    assert!(fixture.bus_rows().is_empty(), "no row was appended");
}

/// Missing body is clap's own required-argument error, exit 2, naming <BODY>.
#[test]
fn beep_without_a_body_is_a_clap_required_argument_error() {
    let fixture = Fixture::new("beep-no-body");
    fixture.write_registry(folded_registry());
    let output = fixture.boop_as("feature-a", &["beep", "native-n1"]);
    assert_eq!(output.status.code(), Some(2), "stdout: {}", stdout(&output));
    let err = stderr(&output);
    assert!(err.contains("<BODY>"), "stderr: {err}");
    assert!(
        err.contains("required arguments were not provided"),
        "stderr: {err}"
    );
}

/// The `beep` group still reaches its own subcommands with the route
/// positional in front of them.
#[test]
fn the_beep_subcommand_group_still_parses_under_the_route_positional() {
    let fixture = Fixture::new("beep-group");
    fixture.write_registry(folded_registry());
    for argv in [
        vec!["beep", "lane", "list"],
        vec!["beep", "pstree"],
        vec!["beep", "ps"],
    ] {
        let output = fixture.boop_as("feature-a", &argv);
        assert!(
            output.status.success(),
            "{argv:?} stderr: {}",
            stderr(&output)
        );
    }
}
