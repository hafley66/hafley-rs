//! End-to-end: the hook inbox, which is what drains a claude coordinator's
//! mail when no claude door answers for its pane. `boop adopt` installs
//! nothing; `boop inbox hooks` is the one verb that writes the project
//! settings, and a hail is never typed into a pane whatever the answer.

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
        std::fs::create_dir_all(root.join("home")).unwrap();
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
            .env("HOME", self.root.join("home"))
            .output()
            .unwrap()
    }

    /// A verb that takes no `--mail-dir`, `inbox hooks` being the one that
    /// speaks about a project directory rather than a mailbox.
    fn boop_no_mail_dir(&self, args: &[&str]) -> std::process::Output {
        Command::new(BOOP)
            .args(args)
            .env("BOOP_DB", self.root.join("boop.db"))
            .env("HOME", self.root.join("home"))
            .output()
            .unwrap()
    }

    /// Write the two hooks into the project settings, the way a coordinator
    /// with no live claude door does it for itself.
    fn install_hooks(&self) -> std::process::Output {
        self.boop_no_mail_dir(&[
            "inbox",
            "hooks",
            "--name",
            &self.name,
            "--cwd",
            self.project().to_str().unwrap(),
        ])
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

/// RECEIPT. `boop inbox hooks` writes the Stop and UserPromptSubmit hooks
/// once, is idempotent (a second Stop hook would deliver every hail twice),
/// and `--uninstall` takes both back out. Adopting the pane writes the route
/// and nothing else: the claude door, not the hook inbox, is where mail goes
/// first, so no adopt touches a project's settings.
#[test]
fn the_inbox_hooks_verb_installs_both_hooks_once_and_removes_them() {
    let coord = Coordinator::new("install");
    let adopted = coord.adopt();
    assert!(
        adopted.status.success(),
        "adopt: {}",
        String::from_utf8_lossy(&adopted.stderr)
    );
    assert!(
        !coord
            .project()
            .join(".claude")
            .join("settings.json")
            .exists(),
        "adopt must not write project settings"
    );

    assert!(coord.install_hooks().status.success());
    let stop = format!("boop inbox drain --as {} --hook stop", coord.name);
    let prompt = format!("boop inbox drain --as {} --hook prompt", coord.name);
    assert_eq!(commands(&coord.settings(), "Stop"), vec![stop.clone()]);
    assert_eq!(
        commands(&coord.settings(), "UserPromptSubmit"),
        vec![prompt.clone()]
    );

    assert!(coord.install_hooks().status.success());
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

/// RECEIPT. With the hook inbox installed and no claude door answering for
/// the pane, two hails sent during one long turn arrive together at the next
/// Stop, exactly once, and the pane is never typed at.
#[test]
fn a_hail_during_a_long_turn_arrives_once_at_the_next_stop_and_never_as_keystrokes() {
    let coord = Coordinator::new("deliver");
    assert!(coord.adopt().status.success());
    assert!(coord.install_hooks().status.success());

    // The long turn: the pane is busy, nothing drains, two hails land.
    let queued = coord.hail("lane fake-lane done rc=0");
    assert!(
        queued.contains("installed inbox hook"),
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

/// The prompt hook prints the same mail as plain context, and one drain is the
/// only drain: whichever hook fires first takes delivery.
#[test]
fn the_prompt_hook_prints_the_mail_as_context_and_takes_delivery() {
    let coord = Coordinator::new("prompt");
    assert!(coord.adopt().status.success());
    assert!(coord.install_hooks().status.success());
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

/// RECEIPT. Taking the hooks out of a coordinator whose claude door answers
/// nothing leaves the hail with nowhere to land: it stays queued on the bus,
/// the refusal names the missing door, and the pane is still never typed at.
/// Pane injection was the old fallback and there is no fallback now.
#[test]
fn removing_the_hooks_leaves_a_hail_queued_with_a_named_refusal() {
    let coord = Coordinator::new("restore");
    assert!(coord.adopt().status.success());
    assert!(coord.install_hooks().status.success());
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
    let queued = coord.hail("back to nobody");
    assert!(
        !queued.contains("installed inbox hook"),
        "the removed hook still routed the hail: {queued}"
    );
    assert!(
        queued.contains("no live claude session for"),
        "the landing line must name the door it tried: {queued}"
    );
    std::thread::sleep(std::time::Duration::from_millis(300));
    let pane = coord.pane();
    assert!(!pane.contains("back to nobody"), "pane: {pane}");
    assert!(
        std::fs::read_to_string(coord.mail().join("bus.ndjson"))
            .unwrap()
            .contains("back to nobody"),
        "an unreachable hail must stay on the bus"
    );
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
        .env("HOME", coord.root.join("home"))
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
