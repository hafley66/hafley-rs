//! `boop me mood` set, read and clear, and the drain path rendering queued
//! mail through the receiver's effective mood rather than a fixed shape.

use boop_store::testing::BoopCommandExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const BOOP: &str = env!("CARGO_BIN_EXE_boop");
static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A private root under the OS temp dir, unique per test run, so no command
/// under it ever opens the machine's live `~/.agent` store or mailbox.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!(
            "boop-session-mood-{}-{}-{tag}",
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

    /// `me mood` conflicts with `--mail-dir` on the `me` struct itself, so this
    /// carries only HOME and BOOP_DB; every other command adds `--mail-dir`.
    fn boop(&self, args: &[&str]) -> Output {
        Command::new(BOOP)
            .args(args)
            .boop_test_root(self.root.join("home"))
            .env("BOOP_DB", self.root.join("boop.db"))
            .output()
            .unwrap()
    }

    fn boop_mail(&self, args: &[&str]) -> Output {
        Command::new(BOOP)
            .args(args)
            .arg("--mail-dir")
            .arg(self.mail())
            .boop_test_root(self.root.join("home"))
            .env("BOOP_DB", self.root.join("boop.db"))
            .output()
            .unwrap()
    }

    fn hail(&self, body: &str) {
        let out = self.boop_mail(&[
            "beep",
            "coord",
            body,
            "--as",
            "lane-x",
            "--kind",
            "result",
            "--no-wait",
        ]);
        assert!(
            out.status.success(),
            "hail stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn drain(&self) -> String {
        let out = self.boop_mail(&["inbox", "drain", "--as", "coord", "--hook", "plain"]);
        assert!(
            out.status.success(),
            "drain stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn me_mood_sets_reads_and_clears_the_named_session() {
    let fixture = Fixture::new("set-read-clear");

    let set = fixture.boop(&["me", "mood", "unga", "--as", "coord"]);
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&set.stdout).trim(),
        "mood: unga (set on coord)",
        "stdout={} stderr={}",
        String::from_utf8_lossy(&set.stdout),
        String::from_utf8_lossy(&set.stderr)
    );

    let read = fixture.boop(&["me", "mood", "--as", "coord"]);
    assert!(
        read.status.success(),
        "{}",
        String::from_utf8_lossy(&read.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&read.stdout).trim(),
        "mood: unga (set by coord)",
        "stdout={} stderr={}",
        String::from_utf8_lossy(&read.stdout),
        String::from_utf8_lossy(&read.stderr)
    );

    let cleared = fixture.boop(&["me", "mood", "--clear", "--as", "coord"]);
    assert!(
        cleared.status.success(),
        "{}",
        String::from_utf8_lossy(&cleared.stderr)
    );
    let cleared_stdout = String::from_utf8_lossy(&cleared.stdout).into_owned();
    let cleared_lines: Vec<&str> = cleared_stdout.trim().lines().collect();
    assert_eq!(
        cleared_lines,
        ["mood cleared on coord", "mood: plain (set by default)"],
        "stdout={cleared_stdout}"
    );

    let cleared_again = fixture.boop(&["me", "mood", "--clear", "--as", "coord"]);
    assert!(
        cleared_again.status.success(),
        "{}",
        String::from_utf8_lossy(&cleared_again.stderr)
    );
    let again_stdout = String::from_utf8_lossy(&cleared_again.stdout).into_owned();
    let again_lines: Vec<&str> = again_stdout.trim().lines().collect();
    assert_eq!(
        again_lines,
        [
            "coord had no mood of its own",
            "mood: plain (set by default)"
        ],
        "stdout={again_stdout}"
    );
}

#[test]
fn me_favorite_follows_the_callers_bound_native_thread() {
    let fixture = Fixture::new("favorite-native-binding");
    let store = boop::Store::open(fixture.root.join("boop.db")).unwrap();
    let session = boop::harness::SessionRef {
        harness: boop::harness::HarnessId::Claude, session_id: "native-thread".into(), nickname: "native-thread".into(),
        path: fixture.root.join("native.jsonl"), cwd: None, git_branch: None, modified_ms: 1, size: 0,
        tmux: None, tmux_socket: None, parent: None,
    };
    store.project_discovered_session(&session).unwrap();
    store.write_turn("native-thread", 1, 1, "assistant", "older native answer", None).unwrap();
    store.write_turn("native-thread", 2, 2, "assistant", "latest native answer", None).unwrap();
    store.connection().execute("INSERT INTO agent_route(route,kind,harness,session_id) VALUES ('caller-route','coordinator','claude','native-thread')", []).unwrap();
    let output = Command::new(BOOP).args(["me", "favorite", "--note", "fixture"])
        .boop_test_root(&fixture.root).env("BOOP_DB", fixture.root.join("boop.db"))
        .env("BOOP_MAIL_DIR", fixture.mail()).env("BOOP_NO_SYNC", "1").env("BOOP_SESSION", "caller-route")
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let source: String = store.connection().query_row("SELECT source FROM agent_favorite", [], |row| row.get(0)).unwrap();
    assert_eq!(source, "claude:native-thread:assistant:2");
}

#[test]
fn an_unknown_mood_name_fails_and_names_the_known_ones() {
    let fixture = Fixture::new("unknown-mood");
    let out = fixture.boop(&["me", "mood", "shouty", "--as", "coord"]);
    assert!(
        !out.status.success(),
        "an unknown mood name must fail: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown mood shouty; known moods: board, plain, unga"),
        "stderr={stderr}"
    );
}

#[test]
fn a_drained_inbox_renders_lane_completion_mail_through_the_receivers_mood() {
    let fixture = Fixture::new("unga-drain");
    let set = fixture.boop(&["me", "mood", "unga", "--as", "coord"]);
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );

    fixture.hail("lane fix/tls done rc=0");
    let printed = fixture.drain();

    assert!(printed.starts_with("boop inbox:\n\n"), "printed={printed}");
    assert!(
        printed.contains("unga: lists/tables/mermaid only, no prose"),
        "printed={printed}"
    );
    assert!(printed.contains("lane-x -> "), "printed={printed}");
    assert!(
        printed.contains("lane fix/tls done rc=0"),
        "printed={printed}"
    );
    assert!(!printed.contains("[boop "), "printed={printed}");
}

#[test]
fn a_drain_with_no_mood_anywhere_keeps_the_default_shape() {
    let fixture = Fixture::new("default-drain");
    fixture.hail("lane fix/tls done rc=0");
    let printed = fixture.drain();

    assert!(printed.contains("[boop "), "printed={printed}");
    assert!(
        printed.contains("from lane-x] lane fix/tls done rc=0"),
        "printed={printed}"
    );
    assert!(!printed.contains("unga:"), "printed={printed}");
}
