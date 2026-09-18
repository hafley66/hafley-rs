//! Real-binary integration for the persistent selection backend: a scratch
//! SQLite store, a scratch tmux server (`TMUX_TMPDIR`, never the user's), and
//! the built `boop` executable. No in-process doubles for the tmux seam.
//!
//! Harness TUIs are not launched here: a full coordinator requires a real
//! claude/codex/opencode/llmock process, out of scope for this backend. Routes
//! use pane-less coordinators to prove recipient selection and safety, and real
//! scratch panes to prove list/focus resolution.

use std::path::PathBuf;
use std::process::Command;

use boop_store::testing::BoopCommandExt;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

/// One scratch root: mail store, tmux socket dir, config. Every tmux client and
/// every boop subprocess runs with `TMUX`/`TMUX_PANE` removed and `TMUX_TMPDIR`
/// pointed at the scratch dir, so the user's default server is never touched.
struct Scratch {
    root: PathBuf,
    mail: PathBuf,
    tmux_tmpdir: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .env("TMUX_TMPDIR", &self.tmux_tmpdir)
            .args(["-L", "default", "kill-server"])
            .output();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Scratch {
    fn new(tag: &str) -> Self {
        // A unix socket path is capped near 104 bytes, and the platform temp
        // dir is long on macOS, so the scratch root lives directly under /tmp.
        let root = PathBuf::from("/tmp").join(format!("bsel-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mail = root.join("mail");
        let tmux_tmpdir = root.join("tmux");
        std::fs::create_dir_all(&mail).unwrap();
        std::fs::create_dir_all(&tmux_tmpdir).unwrap();
        Scratch {
            root,
            mail,
            tmux_tmpdir,
        }
    }

    fn tmux(&self, args: &[&str]) -> std::process::Output {
        Command::new("tmux")
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .env("TMUX_TMPDIR", &self.tmux_tmpdir)
            .args(["-f", "/dev/null"])
            .args(args)
            .output()
            .expect("tmux is installed")
    }

    fn new_session(&self, name: &str) {
        let output = self.tmux(&["new-session", "-d", "-s", name, "sleep 120"]);
        assert!(
            output.status.success(),
            "tmux new-session {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn pane(&self, target: &str) -> String {
        let output = self.tmux(&["list-panes", "-t", target, "-F", "#{pane_id}"]);
        assert!(
            output.status.success(),
            "list-panes {target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .expect("one pane")
            .trim()
            .to_owned()
    }

    fn write_registry(&self, json: &str) {
        std::fs::write(self.mail.join("registry.json"), json).unwrap();
    }

    fn boop(&self, args: &[&str]) -> std::process::Output {
        let mut command = Command::new(BOOP);
        command
            .boop_test_root(&self.root)
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .env("TMUX_TMPDIR", &self.tmux_tmpdir)
            .env("BOOP_NO_SYNC", "1")
            .env("BOOP_DB", self.mail.join("boop.db"))
            .env("BOOP_MAIL_DIR", &self.mail)
            .env("BOOP_CONFIG", self.root.join("config/boop/config.json"))
            .args(args);
        command.output().expect("run boop")
    }

    /// `boop beep selection --mail-dir <mail> <verb...>`; the mailbox flag
    /// belongs to the `selection` group, ahead of the verb.
    fn selection(&self, verb: &[&str]) -> std::process::Output {
        let mut args: Vec<String> = vec![
            "beep".into(),
            "selection".into(),
            "--mail-dir".into(),
            self.mail.display().to_string(),
        ];
        args.extend(verb.iter().map(|arg| (*arg).to_owned()));
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.boop(&refs)
    }

    /// Run one `boop db` query against the scratch store and return stdout.
    fn sql(&self, query: &str) -> String {
        let output = self.boop(&["db", query]);
        assert!(
            output.status.success(),
            "db query failed: {query}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn list_json(&self) -> String {
        let output = self.selection(&["list"]);
        assert!(
            output.status.success(),
            "selection list: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
}

/// RECEIPT. `selection list` is JSON rows for registered harness routes on
/// live panes, recent focus first. Sabotage: sorting by route name alone puts
/// `alpha` ahead of `beta` after `beta` was focused most recently.
#[test]
fn list_is_json_rows_ordered_by_focus() {
    let scratch = Scratch::new("list");
    scratch.new_session("sel-list");
    let first = scratch.pane("sel-list:0");
    scratch.tmux(&["new-window", "-t", "sel-list", "-n", "second"]);
    let second = scratch.pane("sel-list:1");
    scratch.write_registry(&format!(
        r#"{{"alpha":{{"kind":"lane","harness":"claude","tmux":"{first}"}},
             "beta":{{"kind":"lane","harness":"claude","tmux":"{second}"}}}}"#
    ));

    let f1 = scratch.selection(&["focus", &first, "--at", "1000"]);
    assert!(
        f1.status.success(),
        "focus {first}: {}",
        String::from_utf8_lossy(&f1.stderr)
    );
    let f2 = scratch.selection(&["focus", &second, "--at", "2000"]);
    assert!(
        f2.status.success(),
        "focus {second}: {}",
        String::from_utf8_lossy(&f2.stderr)
    );

    let json = scratch.list_json();
    let beta = json.find(r#""route":"beta""#).expect("beta row");
    let alpha = json.find(r#""route":"alpha""#).expect("alpha row");
    assert!(beta < alpha, "most recently focused first: {json}");
    assert!(json.contains(r#""lastFocusedAt":2000"#), "{json}");
    assert!(json.contains(r#""lastFocusedAt":1000"#), "{json}");
    assert!(json.contains(r#""session":"sel-list""#), "{json}");
    assert!(json.contains(r#""kind":"lane""#), "{json}");
    assert!(json.contains(r#""pane":"%"#), "{json}");
    assert!(json.contains(r#""target":"sel-list:0.0""#), "{json}");
    assert!(json.contains(r#""selected":false"#), "{json}");
}

/// RECEIPT. The list is unchanged in scope: every harness route with a live
/// pane is still listed, lane included, each row stamped with its kind and
/// composed target. The Instant dropdown, not the CLI, decides which kinds are
/// recipients. Sabotage: filtering lanes in the CLI hides rows other consumers
/// rely on.
#[test]
fn list_stamps_kind_metadata_and_keeps_every_live_route() {
    let scratch = Scratch::new("kinds");
    scratch.new_session("sel-kinds");
    let coord = scratch.pane("sel-kinds:0");
    scratch.tmux(&["new-window", "-t", "sel-kinds", "-n", "lane"]);
    let lane = scratch.pane("sel-kinds:1");
    scratch.tmux(&["new-window", "-t", "sel-kinds", "-n", "native"]);
    let native = scratch.pane("sel-kinds:2");
    scratch.write_registry(&format!(
        r#"{{"coord":{{"kind":"coordinator","harness":"claude","tmux":"{coord}"}},
             "lane":{{"kind":"lane","harness":"claude","tmux":"{lane}"}},
             "native":{{"kind":"native","harness":"codex","tmux":"{native}"}},
             "paneless-native":{{"kind":"native","harness":"codex"}}}}"#
    ));

    let json = scratch.list_json();
    assert!(json.contains(r#""route":"coord""#), "{json}");
    assert!(json.contains(r#""route":"lane""#), "{json}");
    assert!(json.contains(r#""route":"native""#), "{json}");
    assert!(json.contains(r#""kind":"coordinator""#), "{json}");
    assert!(json.contains(r#""kind":"lane""#), "{json}");
    assert!(json.contains(r#""kind":"native""#), "{json}");
    assert!(
        json.contains(r#""target":"sel-kinds:1.0""#),
        "lane row carries its composed target: {json}"
    );
    assert!(
        !json.contains("paneless-native"),
        "a pane-less route is never listed: {json}"
    );
}

/// RECEIPT. Focus is monotone: a later `--at` moves a route, an earlier one
/// does not. Sabotage: a plain assignment lets the 500 rewinding stamp win.
#[test]
fn focus_is_monotonic() {
    let scratch = Scratch::new("monotone");
    scratch.new_session("sel-monotone");
    let pane = scratch.pane("sel-monotone:0");
    scratch.write_registry(&format!(
        r#"{{"only":{{"kind":"lane","harness":"claude","tmux":"{pane}"}}}}"#
    ));

    assert!(scratch
        .selection(&["focus", &pane, "--at", "1000"])
        .status
        .success());
    assert!(scratch
        .selection(&["focus", &pane, "--at", "500"])
        .status
        .success());
    assert!(
        scratch
            .sql("SELECT 'at=' || last_focused_at FROM agent_route_selection")
            .contains("at=1000"),
        "an earlier stamp must not rewind focus"
    );
    assert!(scratch
        .selection(&["focus", &pane, "--at", "3000"])
        .status
        .success());
    assert!(scratch
        .sql("SELECT 'at=' || last_focused_at FROM agent_route_selection")
        .contains("at=3000"));
}

/// RECEIPT. A bare session target focuses only the route on the session's
/// active window's active pane, not every Boop pane route in the session.
/// Sabotage: focusing each pane in the session stamps both rows.
#[test]
fn session_focus_stamps_only_the_active_pane_route() {
    let scratch = Scratch::new("session-focus");
    scratch.new_session("sel-session");
    let old = scratch.pane("sel-session:0");
    scratch.tmux(&["new-window", "-t", "sel-session", "-n", "second"]);
    let active = scratch.pane("sel-session:1");
    scratch.write_registry(&format!(
        r#"{{"old":{{"kind":"lane","harness":"claude","tmux":"{old}"}},
             "active":{{"kind":"lane","harness":"claude","tmux":"{active}"}}}}"#
    ));

    let output = scratch.selection(&["focus", "sel-session", "--at", "3000"]);
    assert!(
        output.status.success(),
        "session focus: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows = scratch.sql(
        "SELECT route || '=' || COALESCE(CAST(last_focused_at AS TEXT),'null')
           FROM agent_route_selection ORDER BY route",
    );
    assert!(rows.contains("active=3000"), "{rows}");
    assert!(
        !rows.contains("old=3000"),
        "old window must stay unfocused: {rows}"
    );
}

/// RECEIPT. Selection state lives beside the route, not on it: a route
/// re-registration (`lane patch`, an upsert of `agent_route`) keeps
/// the checkbox and focus stamp. Sabotage: a cascading FK erases them.
#[test]
fn selection_survives_a_route_upsert() {
    let scratch = Scratch::new("upsert");
    scratch.new_session("sel-upsert");
    let pane = scratch.pane("sel-upsert:0");
    scratch.write_registry(&format!(
        r#"{{"alpha":{{"kind":"lane","harness":"claude","tmux":"{pane}"}}}}"#
    ));

    assert!(scratch.selection(&["list"]).status.success());
    assert!(scratch
        .selection(&["set", "alpha", "--checked"])
        .status
        .success());
    assert!(scratch
        .selection(&["focus", &pane, "--at", "1234"])
        .status
        .success());

    // A real route rewrite: patch changes the model, so registry_update writes
    // the whole row with ON CONFLICT DO UPDATE.
    let patch = scratch.boop(&[
        "beep",
        "lane",
        "patch",
        "alpha",
        "--harness",
        "claude",
        "--tmux",
        &pane,
        "--model",
        "changed",
        "--mail-dir",
        &scratch.mail.display().to_string(),
    ]);
    assert!(
        patch.status.success(),
        "lane patch: {}",
        String::from_utf8_lossy(&patch.stderr)
    );

    let rows = scratch.sql(
        "SELECT 'selected=' || selected || ' at=' || COALESCE(CAST(last_focused_at AS TEXT),'null')
           FROM agent_route_selection WHERE route='alpha'",
    );
    assert!(rows.contains("selected=1"), "{rows}");
    assert!(rows.contains("at=1234"), "{rows}");
}

/// A route deleted through a connection with legacy FK enforcement off still
/// removes its selection. Reusing the same route name starts unchecked.
#[test]
fn deleted_route_does_not_reuse_selection() {
    let scratch = Scratch::new("delete");
    scratch.write_registry(r#"{"alpha":{"kind":"coordinator","harness":"claude"}}"#);
    assert!(scratch
        .selection(&["set", "alpha", "--checked"])
        .status
        .success());
    let connection = rusqlite::Connection::open(scratch.mail.join("boop.db")).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "OFF")
        .unwrap();
    connection
        .execute("DELETE FROM agent_route WHERE route='alpha'", [])
        .unwrap();
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM agent_route_selection", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
    connection
        .execute(
            "INSERT INTO agent_route(route,kind,harness) VALUES ('alpha','coordinator','claude')",
            [],
        )
        .unwrap();
    let out = scratch.boop(&["beep", "shout", "--selected", "--as", "instant"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no Boop recipients selected"));
}

/// RECEIPT. An explicit `--to` naming an unknown route fails and sends
/// nothing, never widening to the live broadcast. Sabotage: treating the
/// emptied explicit set as a bare broadcast writes two rows.
#[test]
fn explicit_unknown_recipients_never_fall_back_to_broadcast() {
    let scratch = Scratch::new("explicit-stale");
    scratch.write_registry(
        r#"{"live1":{"kind":"coordinator","harness":"codex"},
            "live2":{"kind":"coordinator","harness":"claude"}}"#,
    );

    let output = scratch.boop(&[
        "beep",
        "shout",
        "--to",
        "ghost",
        "--as",
        "instant",
        "--mail-dir",
        &scratch.mail.display().to_string(),
    ]);
    assert!(!output.status.success(), "unknown target must fail");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("no-route ghost"), "{stdout}");
    assert!(
        scratch
            .sql("SELECT 'count=' || COUNT(*) FROM agent_mail")
            .contains("count=0"),
        "no broadcast row may be written"
    );
}

/// RECEIPT. `shout --selected` errors when nothing is selected and never
/// broadcasts. Sabotage: an empty selection treated as "no filter" hits both
/// live routes.
#[test]
fn empty_selection_errors_without_broadcast() {
    let scratch = Scratch::new("empty-selected");
    scratch.write_registry(
        r#"{"live1":{"kind":"coordinator","harness":"codex"},
            "live2":{"kind":"coordinator","harness":"claude"}}"#,
    );

    let output = scratch.boop(&[
        "beep",
        "shout",
        "--selected",
        "--mail-dir",
        &scratch.mail.display().to_string(),
    ]);
    assert!(!output.status.success(), "empty selection must fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no Boop recipients selected"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(scratch
        .sql("SELECT 'count=' || COUNT(*) FROM agent_mail")
        .contains("count=0"));
}

/// RECEIPT. `shout --selected` sends to exactly the selected route and the
/// `--as` sender is honored even when it is not a registered route.
/// Sabotage: an unregistered `--as instant` downgraded to `coordinator`, or the
/// unselected route also receiving a row.
#[test]
fn selected_sends_only_to_the_selected_route() {
    let scratch = Scratch::new("selected");
    scratch.write_registry(
        r#"{"live1":{"kind":"coordinator","harness":"codex"},
            "live2":{"kind":"coordinator","harness":"claude"}}"#,
    );

    assert!(scratch
        .selection(&["set", "live2", "--checked"])
        .status
        .success());
    let output = scratch.boop(&[
        "beep",
        "shout",
        "--selected",
        "--as",
        "instant",
        "--mail-dir",
        &scratch.mail.display().to_string(),
    ]);
    assert!(
        output.status.success(),
        "selected shout: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let rows = scratch.sql("SELECT to_route || '|' || from_route FROM agent_mail");
    assert!(rows.contains("live2|instant"), "{rows}");
    assert!(
        !rows.contains("live1|"),
        "unselected route got a row: {rows}"
    );
}

/// RECEIPT. With no tmux server, `selection list` is a usable status, not a
/// failure: exit 0, an empty JSON array, and a stderr note. Sabotage: bailing
/// on the missing server makes the UI read an outage as a crash.
#[test]
fn list_without_a_tmux_server_is_an_empty_status() {
    let scratch = Scratch::new("no-server");
    // No tmux server is started on this scratch socket dir.
    let output = scratch.selection(&["list"]);
    assert!(
        output.status.success(),
        "no server must not fail: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "[]");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no tmux server"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
