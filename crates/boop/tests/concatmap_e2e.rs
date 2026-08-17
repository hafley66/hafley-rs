//! concatmap e2e: feed two bundles from the window query into a live model,
//! then assert the receipts (the one-way user turns we sent) and liveness (at
//! least one assistant turn) as facts in the boop store.
//!
//! Gated: this spends real model calls, so it is `#[ignore]`d. Run it with
//! `cargo test -p boop --test concatmap_e2e -- --ignored --nocapture`.
//!
//! The only things asserted are receipts. Model *output* is never compared:
//! the user turn is deterministic (we control the bundle text byte-for-byte),
//! so it is asserted exactly; the assistant turn is only asserted as ">= 1".
//!
//! The receipts are scoped to the mapper session alone (matched by cwd), so
//! the resident `db sync` ingesting this machine's other transcripts cannot
//! satisfy the assertion.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use boop::concatmap::trim_double_encoded;
use boop::ident::{Store, TurnQuery};
use boop::rows::{SessionRow, TurnRow};

/// The fallback chain: the first model that yields a receipt + reply wins.
const MODEL_CHAIN: &[&str] = &[
    "openrouter/deepseek/deepseek-v4-flash-0731",
    "google/gemini-3.7-preview",
    "claude-haiku-4-5-20251001",
    "gpt-5.6-luna@medium",
];

const BUNDLES: &[&str] = &["bundle one", "bundle two"];

/// One row per user turn in the source session; the loop sends each verbatim.
const WINDOW_SQL: &str = "SELECT t.turn AS id, t.ts AS ts, t.said AS text
    FROM agent_turn t
    JOIN dict_session s ON s.id = t.session_id
    JOIN dict_role r ON r.id = t.role_id
    WHERE s.value = :session AND r.value = 'user' AND t.ts > :cursor
    ORDER BY t.ts";

const RECEIPT_TIMEOUT: Duration = Duration::from_secs(60);

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_boop")
}

struct Fixture {
    root: PathBuf,
    db: PathBuf,
    state: PathBuf,
    rules: PathBuf,
    sync: Option<Child>,
    mapper: Option<Child>,
    mapper_pid: Option<u32>,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("boop-cm-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Fixture {
            db: root.join("boop.db"),
            state: root.join("state"),
            rules: root.join("rules.json"),
            root,
            sync: None,
            mapper: None,
            mapper_pid: None,
        }
    }

    /// Seed the source session: two user turns, no `agent_session` row, so the
    /// receipts (turn queries join `agent_session`) see only the mapper's turns.
    fn seed(&self) {
        let store = Store::open(self.db.clone()).unwrap();
        store
            .write_turn("ses_src", 1, 10, "user", BUNDLES[0])
            .unwrap();
        store
            .write_turn("ses_src", 2, 11, "user", BUNDLES[1])
            .unwrap();
    }

    fn write_rules(&self) {
        let rules = serde_json::json!({
            "feed": "chat",
            "goal": "Reply with exactly the word ok and nothing else, to every message you receive.",
            "window": WINDOW_SQL
        });
        std::fs::write(&self.rules, rules.to_string()).unwrap();
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(bin());
        cmd.env("BOOP_DB", &self.db);
        cmd.args(args);
        cmd
    }

    fn start_sync(&mut self) {
        let err = std::fs::File::create(self.root.join("sync.stderr")).unwrap();
        self.sync = Some(
            self.command(&["db", "sync", "create", "--forever"])
                .stdout(Stdio::null())
                .stderr(Stdio::from(err))
                .spawn()
                .expect("spawn db sync"),
        );
    }

    fn start_mapper(&mut self, model: &str) {
        let _ = std::fs::remove_dir_all(&self.state);
        let err = std::fs::File::create(self.root.join("concatmap.stderr")).unwrap();
        let mut command = self.command(&[
            "concatmap",
            "--session",
            "ses_src",
            "--rules",
            self.rules.to_str().unwrap(),
            "--state",
            self.state.to_str().unwrap(),
            "--model",
            model,
            "--poll-secs",
            "1",
        ]);
        // No inherited tmux pane: the TUI opens its own `boop-tui-<pid>`
        // session instead of hijacking the host's, so teardown can kill it.
        command
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .env_remove("TMUX_PANE_PID");
        let child = command
            .stdout(Stdio::null())
            .stderr(Stdio::from(err))
            .spawn()
            .expect("spawn concatmap");
        self.mapper_pid = Some(child.id());
        self.mapper = Some(child);
    }

    fn kill_mapper(&mut self) {
        if let Some(mut child) = self.mapper.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // The TUI opened its own `boop-tui-<pid>` session; kill it so the
        // resident opencode (a multi-GB process) does not outlive the test.
        if let Some(pid) = self.mapper_pid.take() {
            let _ = Command::new("tmux")
                .args(["kill-session", "-t", &format!("boop-tui-{pid}")])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }

    fn sessions(&self) -> Vec<SessionRow> {
        let store = Store::open_readonly(self.db.clone()).unwrap();
        store.session_rows(None, None).unwrap()
    }

    fn turns_in(&self, session: &str, role: &str) -> Vec<TurnRow> {
        let store = Store::open_readonly(self.db.clone()).unwrap();
        store
            .turn_rows(&TurnQuery {
                session: Some(session.to_owned()),
                role: Some(role.to_owned()),
                ..Default::default()
            })
            .unwrap()
    }

    /// The two bundle texts among the mapper's user turns, in order, with the
    /// goal turn (and anything else) filtered out.
    fn mapper_bundles(&self, session: &str) -> Vec<String> {
        self.turns_in(session, "user")
            .iter()
            .map(|row| trim_double_encoded(&row.said).trim().to_owned())
            .filter(|text| BUNDLES.contains(&text.as_str()))
            .collect()
    }

    /// Liveness: the mapper produced at least one non-empty assistant turn.
    fn mapper_has_reply(&self, session: &str) -> bool {
        self.turns_in(session, "assistant")
            .iter()
            .any(|row| !trim_double_encoded(&row.said).trim().is_empty())
    }

    /// The mapper session, matched by cwd against the canonicalized state dir
    /// (the same spelling the loop's own `mapper_session` uses).
    fn mapper_session_id(&self) -> Option<String> {
        let canon = std::fs::canonicalize(&self.state)
            .ok()?
            .display()
            .to_string();
        self.sessions()
            .into_iter()
            .find(|s| s.cwd.as_deref() == Some(canon.as_str()))
            .map(|s| s.session)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.kill_mapper();
        if let Some(mut child) = self.sync.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Poll `probe` until it yields a value or `timeout` elapses.
fn wait_for_value<T>(timeout: Duration, mut probe: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = probe() {
            return Some(value);
        }
        if Instant::now() >= deadline {
            return probe();
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Wait for `pred` to hold, polling every 500ms, until `timeout`.
fn wait_until(timeout: Duration, mut pred: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if pred() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    pred()
}

/// RECEIPT. Two bundles from the window query reach a live model as exactly two
/// user turns, byte-for-byte, and the model answers at least once. Tries each
/// model in the chain until one delivers both.
#[test]
#[ignore = "spends real model calls; run with --ignored"]
fn two_bundles_land_as_user_turns_and_the_model_answers() {
    let mut fixture = Fixture::new();
    fixture.seed();
    fixture.write_rules();
    fixture.start_sync();

    for model in MODEL_CHAIN {
        eprintln!("[e2e] trying model {model}");
        fixture.start_mapper(model);

        // The mapper session must be discovered and projected before either
        // receipt can be read.
        let Some(mapper) = wait_for_value(Duration::from_secs(30), || fixture.mapper_session_id())
        else {
            eprintln!("[e2e] mapper session never appeared for {model}");
            fixture.kill_mapper();
            continue;
        };

        // The receipt and liveness together: both bundles delivered as user
        // turns, and the model produced a non-empty reply.
        let delivered = wait_until(RECEIPT_TIMEOUT, || {
            fixture.mapper_bundles(&mapper) == BUNDLES && fixture.mapper_has_reply(&mapper)
        });
        eprintln!(
            "[e2e] {model}: bundles={:?} reply={}",
            fixture.mapper_bundles(&mapper),
            fixture.mapper_has_reply(&mapper)
        );
        if !delivered {
            fixture.kill_mapper();
            continue;
        }

        assert_eq!(
            fixture.mapper_bundles(&mapper),
            BUNDLES,
            "exactly two user turns delivered, one-way, in order"
        );
        assert!(fixture.mapper_has_reply(&mapper), "the model answered");
        return; // success on this model
    }

    panic!(
        "no model in the chain delivered two user turns and a reply: {:?}",
        MODEL_CHAIN
    );
}
