use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use boop::Store;
use wait_timeout::ChildExt;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn agent_summary_reads_while_a_wal_writer_holds_its_transaction() {
    let root = std::env::temp_dir().join(format!(
        "boop-agent-summary-contention-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("mail")).unwrap();
    let db = root.join("boop.db");
    let writer = Store::open(db.clone()).unwrap();
    writer.begin().unwrap();

    let mut child = Command::new(BOOP)
        .args(["agent", "summary", "--format", "json", "--mail-dir"])
        .arg(root.join("mail"))
        .env("BOOP_DB", &db)
        .env("HOME", &root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let status = child
        .wait_timeout(std::time::Duration::from_secs(5))
        .unwrap();

    writer.rollback().unwrap();
    let output = if status.is_none() {
        child.kill().unwrap();
        child.wait_with_output().unwrap()
    } else {
        child.wait_with_output().unwrap()
    };
    assert!(
        status.is_some() && output.status.success(),
        "agent summary must complete while the writer holds its transaction; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    drop(writer);
    let _ = std::fs::remove_dir_all(root);
}
