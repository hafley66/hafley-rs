//! End-to-end: a hail to a claude coordinator reaches it at the next turn
//! boundary and never through its keyboard.
//!
//! FAIL-PRE-FIX: `boop hail` typed every coordinator-kind route's mail into its
//! pane, so a hail mid-turn landed in whatever the model had open: a tool call,
//! a dialog, a half-typed line. The user's word, 2026-08-16: "we need a
//! different mail system, interrupting the enter key and dialog is a bit noisy".

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const BOOP: &str = env!("CARGO_BIN_EXE_boop");
/// A clock reading is not an identifier (sprefa failure ledger 54).
static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// One coordinator: a project dir, a mailbox, a real tmux pane, and a store of
/// its own so the suite never writes the machine's live ~/.agent/boop.db.
struct Coordinator {
    root: PathBuf,
    name: String,
    session: String,
}

impl Coordinator {
    fn new(tag: &str) -> Coordinator {
        let root = std::env::temp_dir().join(format!(
            "boop-inbox-hooks-{}-{}-{tag}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("mail")).unwrap();
        std::fs::create_dir_all(root.join("project")).unwrap();
        let session = format!("boop-inbox-{}-{tag}", std::process::id());
        let _ = tmux(&["kill-session", "-t", &session]);
        let started = tmux(&["new-session", "-d", "-s", &session]);
        assert!(
            started.status.success(),
            "tmux new-session: {}",
            String::from_utf8_lossy(&started.stderr)
        );
        Coordinator {
            root,
            name: format!("coord-{tag}"),
            session,
        }
    }

    fn project(&self) -> PathBuf {
        self.root.join("project")
    }

    fn mail(&self) -> PathBuf {
        self.root.join("mail")
    }

    fn settings(&self) -> serde_json::Value {
        let path = self.project().join(".claude").join("settings.json");
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        serde_json::from_str(&raw).unwrap()
    }

    fn boop(&self, args: &[&str]) -> std::process::Output {
        Command::new(BOOP)
            .args(args)
            .arg("--mail-dir")
            .arg(self.mail())
            // The suite must never write the machine's live store: it is 374MB
            // with `journal_mode=delete`, and live boop processes hold write
            // locks past the 5s busy_timeout.
            .env("BOOP_DB", self.root.join("boop.db"))
            .output()
            .unwrap()
    }

    /// A verb that takes no `--mail-dir`, `inbox hooks` being the one that
    /// speaks about a project directory rather than a mailbox.
    fn boop_no_mail_dir(&self, args: &[&str]) -> std::process::Output {
        Command::new(BOOP)
            .args(args)
            .env("BOOP_DB", self.root.join("boop.db"))
            .output()
            .unwrap()
    }

    fn adopt(&self) -> std::process::Output {
        self.boop(&[
            "adopt",
            "--name",
            &self.name,
            "--tmux",
            &self.session,
            "--harness",
            "claude",
            "--cwd",
            self.project().to_str().unwrap(),
        ])
    }

    fn hail(&self, body: &str) -> String {
        let out = self.boop(&[
            "hail",
            "--to",
            &self.name,
            "--from",
            "fake-lane",
            "--kind",
            "result",
            "--body",
            body,
        ]);
        assert!(
            out.status.success(),
            "hail: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn drain(&self, hook: &str) -> String {
        let out = self.boop(&["inbox", "drain", "--as", &self.name, "--hook", hook]);
        assert!(
            out.status.success(),
            "drain: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn pane(&self) -> String {
        let captured = tmux(&["capture-pane", "-p", "-t", &self.session]);
        String::from_utf8_lossy(&captured.stdout).into_owned()
    }
}

impl Drop for Coordinator {
    fn drop(&mut self) {
        let _ = tmux(&["kill-session", "-t", &self.session]);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn tmux(args: &[&str]) -> std::process::Output {
    Command::new("tmux").args(args).output().unwrap()
}

fn commands(settings: &serde_json::Value, event: &str) -> Vec<String> {
    settings["hooks"][event]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .flat_map(|group| {
            group["hooks"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(|entry| entry["command"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .collect()
}

// FAIL-PRE-FIX: adopting a claude coordinator wrote a route and nothing else, so
// its mail had no door but the keyboard.
#[test]
fn adopting_a_claude_coordinator_installs_both_hooks_once() {
    let coord = Coordinator::new("install");
    let first = coord.adopt();
    assert!(
        first.status.success(),
        "adopt: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let stop = format!("boop inbox drain --as {} --hook stop", coord.name);
    let prompt = format!("boop inbox drain --as {} --hook prompt", coord.name);
    assert_eq!(commands(&coord.settings(), "Stop"), vec![stop.clone()]);
    assert_eq!(
        commands(&coord.settings(), "UserPromptSubmit"),
        vec![prompt.clone()]
    );

    // Idempotent: adopting the same pane twice is the normal case, and a second
    // Stop hook would deliver every hail twice.
    assert!(coord.adopt().status.success());
    assert_eq!(commands(&coord.settings(), "Stop"), vec![stop]);
    assert_eq!(
        commands(&coord.settings(), "UserPromptSubmit"),
        vec![prompt]
    );

    let removed = coord.boop(&[
        "adopt",
        "--name",
        &coord.name,
        "--tmux",
        &coord.session,
        "--cwd",
        coord.project().to_str().unwrap(),
        "--uninstall-hooks",
    ]);
    assert!(
        removed.status.success(),
        "uninstall: {}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(
        coord.settings().get("hooks").is_none(),
        "settings still hold hooks: {}",
        coord.settings()
    );
}

// FAIL-PRE-FIX, the whole card: a hail to an adopted coordinator was typed into
// its pane mid-turn. Sabotage receipt: removing the `installed_for` branch from
// `deliver_hail` FAILED this with "injected into tmux" on stdout and the body
// visible in `capture-pane`.
#[test]
fn a_hail_during_a_long_turn_arrives_once_at_the_next_stop_and_never_as_keystrokes() {
    let coord = Coordinator::new("deliver");
    assert!(coord.adopt().status.success());

    // The long turn: the pane is busy, nothing drains, two hails land.
    let queued = coord.hail("lane fake-lane done rc=0");
    assert!(
        queued.contains("hook inbox drains it"),
        "hail did not queue for the hook inbox: {queued}"
    );
    assert!(
        !queued.contains("injected into tmux"),
        "hail typed into the pane: {queued}"
    );
    coord.hail("second hail while the turn ran");

    // Turn end: the Stop hook drains, and the model reads the mail as input.
    let payload: serde_json::Value = serde_json::from_str(coord.drain("stop").trim())
        .unwrap_or_else(|error| panic!("Stop hook output is not JSON: {error}"));
    assert_eq!(payload["decision"], "block");
    let reason = payload["reason"].as_str().unwrap();
    assert!(reason.starts_with("boop inbox:\n\n"), "{reason}");
    assert!(reason.contains("lane fake-lane done rc=0"), "{reason}");
    assert!(
        reason.contains("second hail while the turn ran"),
        "both hails must arrive in the one batch: {reason}"
    );
    assert_eq!(
        reason.matches("lane fake-lane done rc=0").count(),
        1,
        "the first hail arrived more than once: {reason}"
    );

    // Once, not once per turn: the next Stop is silent.
    assert_eq!(coord.drain("stop"), "", "a drained hail came back");
    assert_eq!(coord.drain("prompt"), "");

    // Delivery is recorded on the bus, not only in the ledger, so a blocking
    // `boop wait --me` does not replay what the hook already handed over. The
    // interim shell hooks could not do this half and a wait replayed the mail.
    let waited = coord.boop(&["wait", "--me", "--as", &coord.name, "--wait-timeout", "1"]);
    assert_eq!(
        waited.status.code(),
        Some(124),
        "a wait replayed drained mail: {}",
        String::from_utf8_lossy(&waited.stdout)
    );

    // The pane was never typed at.
    let pane = coord.pane();
    assert!(
        !pane.contains("lane fake-lane done rc=0"),
        "the pane received keystrokes: {pane}"
    );
    assert!(
        !pane.contains("second hail"),
        "the pane was typed at: {pane}"
    );
    // Mail is rendered through the receiver's mood, so no fixed prefix marks an
    // injected line; the id every mood may name is what survives that.
    let queued_id = queued
        .split_whitespace()
        .find(|word| word.starts_with("m-"))
        .expect("the queue line names the message id");
    assert!(
        !pane.contains(queued_id),
        "an injected line reached the pane: {pane}"
    );
}

// The prompt hook prints the same mail as plain context, and one drain is the
// only drain: whichever hook fires first takes delivery.
#[test]
fn the_prompt_hook_prints_the_mail_as_context_and_takes_delivery() {
    let coord = Coordinator::new("prompt");
    assert!(coord.adopt().status.success());
    coord.hail("read this before your next prompt");
    let printed = coord.drain("prompt");
    assert!(printed.starts_with("boop inbox:\n\n"), "{printed}");
    assert!(
        printed.contains("read this before your next prompt"),
        "{printed}"
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(printed.trim()).is_err(),
        "the prompt hook must print context, not a decision object: {printed}"
    );
    assert_eq!(coord.drain("stop"), "", "the prompt hook did not ack");
}

// FAIL-PRE-FIX: taking the hooks out has to restore pane injection, or a
// coordinator that opted out would silently stop receiving mail.
#[test]
fn removing_the_hooks_restores_pane_injection() {
    let coord = Coordinator::new("restore");
    assert!(coord.adopt().status.success());
    assert!(coord
        .boop_no_mail_dir(&[
            "inbox",
            "hooks",
            "--name",
            &coord.name,
            "--cwd",
            coord.project().to_str().unwrap(),
            "--uninstall",
        ])
        .status
        .success());
    let queued = coord.hail("back to the keyboard");
    assert!(
        !queued.contains("hook inbox drains it"),
        "the removed hook still routed the hail: {queued}"
    );
    assert!(
        queued.contains("injected into tmux"),
        "pane injection was not restored: {queued}"
    );
    std::thread::sleep(std::time::Duration::from_millis(300));
    let pane = coord.pane();
    assert!(pane.contains("back to the keyboard"), "pane: {pane}");
}

// A lane keeps the mailbox poll: its supervisor reads the bus directly, and no
// hook belongs on a pane running a supervisor.
#[test]
fn a_lane_patch_installs_no_hooks() {
    let coord = Coordinator::new("lane");
    let patched = coord.boop(&[
        "beep",
        "lane",
        "patch",
        "some-lane",
        "--tmux",
        &coord.session,
        "--harness",
        "claude",
        "--cwd",
        coord.project().to_str().unwrap(),
    ]);
    assert!(
        patched.status.success(),
        "lane patch: {}",
        String::from_utf8_lossy(&patched.stderr)
    );
    assert!(
        !coord
            .project()
            .join(".claude")
            .join("settings.json")
            .exists(),
        "a lane must not get a hook inbox"
    );
}

/// `--no-hooks` keeps a claude coordinator on pane injection, for a pane whose
/// project settings nobody wants touched.
#[test]
fn no_hooks_adopts_without_touching_the_project_settings() {
    let coord = Coordinator::new("nohooks");
    let adopted = coord.boop(&[
        "adopt",
        "--name",
        &coord.name,
        "--tmux",
        &coord.session,
        "--harness",
        "claude",
        "--cwd",
        coord.project().to_str().unwrap(),
        "--no-hooks",
    ]);
    assert!(
        adopted.status.success(),
        "adopt: {}",
        String::from_utf8_lossy(&adopted.stderr)
    );
    assert!(!coord
        .project()
        .join(".claude")
        .join("settings.json")
        .exists());
    assert!(coord.hail("typed as before").contains("injected into tmux"));
}

/// The drain reads its own name from the identity ladder when `--as` is absent,
/// the way a hook installed for one coordinator runs inside that coordinator.
#[test]
fn a_drain_without_a_name_uses_the_identity_ladder() {
    let coord = Coordinator::new("ladder");
    assert!(coord.adopt().status.success());
    coord.hail("ladder mail");
    let out = Command::new(BOOP)
        .args(["inbox", "drain", "--hook", "plain"])
        .arg("--mail-dir")
        .arg(coord.mail())
        .env("BOOP_DB", coord.root.join("boop.db"))
        .env("BOOP_SESSION", &coord.name)
        .env_remove("BOOP_LANE")
        .env_remove("BOOP_PARENT")
        .env_remove("BOOP_HARNESS")
        .env_remove("CODEX_THREAD_ID")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "drain: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let printed = String::from_utf8_lossy(&out.stdout);
    assert!(printed.contains("ladder mail"), "{printed}");
}

/// An empty inbox prints nothing and exits 0: the hook runs on every turn.
#[test]
fn an_empty_inbox_is_silent() {
    let coord = Coordinator::new("silent");
    assert!(coord.adopt().status.success());
    assert_eq!(coord.drain("stop"), "");
    assert_eq!(coord.drain("prompt"), "");
    assert!(Path::new(&coord.mail()).exists());
}
