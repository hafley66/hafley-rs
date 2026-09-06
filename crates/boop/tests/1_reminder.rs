use boop_store::{bus, Store};
use std::{
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
const BOOP: &str = env!("CARGO_BIN_EXE_boop");
static SEQ: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "boop-reminder-cli-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(BOOP);
        cmd.args(args)
            .args(["--mail-dir"])
            .arg(&self.dir)
            .env("HOME", &self.dir)
            .env("BOOP_DB", self.dir.join("boop.db"))
            .env("BOOP_CODEX_STATE_DB", self.dir.join("absent-codex.db"))
            .env("BOOP_SESSION", "fixture")
            .env("BOOP_NO_SYNC", "1");
        cmd
    }
    fn run(&self, args: &[&str]) -> Output {
        let out = self.command(args).output().unwrap();
        assert!(
            out.status.success(),
            "{:?}\n{}\n{}",
            args,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }
    fn store(&self) -> Store {
        bus::open_store(&self.dir).unwrap()
    }
    fn add(&self) {
        self.run(&[
            "beep",
            "remind",
            "add",
            "tick",
            "recipient",
            "fixture bounded body",
            "--every",
            "30m",
            "--until",
            "4000000000",
        ]);
    }
    fn due(&self) {
        self.store()
            .connection()
            .execute("UPDATE agent_reminder SET next_ms=0", [])
            .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
#[test]
fn register_session_schedule_list_cancel_and_restart_receipt() {
    let f = Fixture::new();
    f.run(&[
        "beep",
        "agent",
        "register",
        "recipient",
        "--kind",
        "coordinator",
        "--harness",
        "codex",
        "--session",
        "disposable-fixture",
    ]);
    assert_eq!(
        bus::read_routes(&f.dir).unwrap()["recipient"]
            .session_id
            .as_deref(),
        Some("disposable-fixture")
    );
    f.add();
    f.due();
    let first = f.run(&["beep", "remind", "run", "--once"]);
    let store = f.store();
    let rows = bus::messages_in(&store).unwrap();
    assert_eq!(rows.len(), 1);
    let id = &rows[0].id;
    assert_eq!(
        store
            .delivery_rows(id)
            .unwrap()
            .iter()
            .map(|r| r.outcome.as_str())
            .collect::<Vec<_>>(),
        vec!["appended", "held-for-turn-boundary"]
    );
    f.due();
    f.run(&["beep", "remind", "run", "--once"]);
    assert_eq!(bus::messages_in(&store).unwrap().len(), 1);
    let list = f.run(&["beep", "remind", "list"]);
    assert!(String::from_utf8_lossy(&list.stdout).contains(id));
    f.run(&["beep", "remind", "cancel", "tick"]);
    f.run(&["beep", "remind", "run", "--once"]);
    assert!(bus::held_messages(&store, "recipient").unwrap().is_empty());
    println!(
        "CLI fixture receipt: {}",
        String::from_utf8_lossy(&first.stdout)
    );
}
#[test]
fn absent_dead_and_unsafe_modes_never_spawn_or_append() {
    for state in ["absent", "dead", "acpx"] {
        let f = Fixture::new();
        f.run(&["beep", "agent", "register", "recipient"]);
        f.add();
        f.due();
        if state == "dead" {
            f.store()
                .connection()
                .execute(
                    "UPDATE agent_route SET kind='lane',tmux='boop-reminder-nonexistent-fixture'",
                    [],
                )
                .unwrap();
        } else if state == "absent" {
            f.store()
                .connection()
                .execute("DELETE FROM agent_route", [])
                .unwrap();
        } else {
            f.store()
                .connection()
                .execute("UPDATE agent_route SET mode='acpx'", [])
                .unwrap();
        }
        f.run(&["beep", "remind", "run", "--once"]);
        assert!(bus::messages_in(&f.store()).unwrap().is_empty());
        assert!(f.store().reminders().unwrap()[0]
            .detail
            .contains("unavailable"));
    }
}
#[test]
fn exclusive_runner_lock_and_expiry() {
    let f = Fixture::new();
    f.run(&["beep", "agent", "register", "recipient"]);
    f.add();
    f.due();
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(f.dir.join("reminder.lock"))
        .unwrap();
    lock.lock().unwrap();
    let rejected = f
        .command(&["beep", "remind", "run", "--once"])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("another reminder runner"));
    drop(lock);
    f.store()
        .connection()
        .execute("UPDATE agent_reminder SET until_ms=0", [])
        .unwrap();
    f.run(&["beep", "remind", "run", "--once"]);
    assert_eq!(f.store().reminders().unwrap()[0].state, "expired");
    assert!(bus::messages_in(&f.store()).unwrap().is_empty());
}
