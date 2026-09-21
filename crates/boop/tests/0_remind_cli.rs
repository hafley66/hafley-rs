use boop::harness::{HarnessId, SessionRef};
use boop_store::ident::Store;
use boop_store::testing::BoopCommandExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const BOOP: &str = env!("CARGO_BIN_EXE_boop");
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    db: PathBuf,
    mail: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "boop-remind-{}-{}-{name}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mail = root.join("mail");
        std::fs::create_dir_all(&mail).unwrap();
        Self {
            db: root.join("boop.db"),
            root,
            mail,
        }
    }

    fn seed_turns(&self) {
        let store = Store::open(self.db.clone()).unwrap();
        for session in ["ses-latest", "other-session"] {
            store
                .project_discovered_session(&SessionRef {
                    harness: HarnessId::Opencode,
                    session_id: session.to_owned(),
                    nickname: session.to_owned(),
                    path: self.root.join(format!("{session}.jsonl")),
                    cwd: None,
                    git_branch: None,
                    modified_ms: 0,
                    size: 0,
                    tmux: None,
                    tmux_socket: None,
                    parent: None,
                })
                .unwrap();
        }
        store
            .write_turn("ses-latest", 1, 100, "user", "old user", None)
            .unwrap();
        store
            .write_turn("ses-latest", 2, 110, "assistant", "answer", None)
            .unwrap();
        store
            .write_turn("ses-latest", 3, 120, "user", "middle user", None)
            .unwrap();
        store
            .write_turn("ses-latest", 4, 130, "tool", "tool output", None)
            .unwrap();
        store
            .write_turn("ses-latest", 5, 140, "user", "newest user", None)
            .unwrap();
        store
            .write_turn("other-session", 1, 150, "user", "unrelated user", None)
            .unwrap();
    }

    fn tracked_registry(&self) {
        std::fs::write(
            self.mail.join("registry.json"),
            r#"{"caller":{"kind":"native","harness":"opencode","sessionId":"ses-latest"}}"#,
        )
        .unwrap();
    }

    fn run(&self, session: Option<&str>, count: &str) -> Output {
        let mut command = Command::new(BOOP);
        command
            .boop_test_root(&self.root)
            .env("BOOP_DB", &self.db)
            .env("BOOP_MAIL_DIR", &self.mail)
            .args(["remind", count]);
        match session {
            Some(session) => {
                command.env("BOOP_SESSION", session);
            }
            None => {
                command.env_remove("BOOP_SESSION");
            }
        }
        command.output().unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn remind_rejects_a_missing_caller_before_opening_the_store() {
    let fixture = Fixture::new("missing-session");
    let output = fixture.run(None, "2");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no caller session resolved"));
}

#[test]
fn remind_rejects_an_untracked_caller_even_when_that_id_has_rows() {
    let fixture = Fixture::new("untracked-session");
    fixture.seed_turns();
    let output = fixture.run(Some("other-session"), "1");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("is not tracked"));
    assert!(output.stdout.is_empty());
}

#[test]
fn remind_rejects_zero_before_identity_or_store_access() {
    let fixture = Fixture::new("zero-count");
    let output = fixture.run(None, "0");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("count must be positive"));
}

#[test]
fn remind_prints_exact_last_user_window_for_the_tracked_route() {
    let fixture = Fixture::new("success");
    fixture.seed_turns();
    fixture.tracked_registry();
    let output = fixture.run(Some("caller"), "2");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "[user turn 3]\nmiddle user\n\n[user turn 5]\nnewest user\n\n"
    );
}
