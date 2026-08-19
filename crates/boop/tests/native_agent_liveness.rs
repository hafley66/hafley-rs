//! Binary receipts for pane-less native registration and explicit completion.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

fn mail_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "boop-native-liveness-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(dir.join("home")).unwrap();
    dir
}

fn boop(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(BOOP)
        .args(args)
        .arg("--mail-dir")
        .arg(dir)
        .env("HOME", dir.join("home"))
        .env("BOOP_DB", dir.join("boop.db"))
        .output()
        .unwrap()
}

#[test]
fn native_route_stays_live_until_done_and_wait_me_consumes_one_completion() {
    let dir = mail_dir("completion");
    let registered = boop(
        &dir,
        &[
            "beep",
            "agent",
            "register",
            "native-child",
            "--parent",
            "coordinator",
        ],
    );
    assert!(
        registered.status.success(),
        "stderr: {:?}",
        registered.stderr
    );

    let listed = boop(&dir, &["beep", "lane", "list"]);
    let listing = String::from_utf8_lossy(&listed.stdout);
    assert!(listed.status.success(), "stderr: {:?}", listed.stderr);
    assert!(listing.contains("live") && listing.contains("native-child"));

    let lane_wait = Command::new(BOOP)
        .args([
            "beep",
            "lane",
            "wait",
            "native-child",
            "--timeout",
            "1",
            "--mail-dir",
        ])
        .arg(&dir)
        .env("HOME", dir.join("home"))
        .env("BOOP_DB", dir.join("boop.db"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let lane_wait_output = lane_wait.wait_with_output().unwrap();
    assert_eq!(lane_wait_output.status.code(), Some(124));

    let inbox_wait = Command::new(BOOP)
        .args([
            "wait",
            "--me",
            "--as",
            "coordinator",
            "--wait-timeout",
            "5",
            "--mail-dir",
        ])
        .arg(&dir)
        .env("HOME", dir.join("home"))
        .env("BOOP_DB", dir.join("boop.db"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));

    let completed = boop(
        &dir,
        &["beep", "agent", "done", "native-child", "--rc", "7"],
    );
    assert!(completed.status.success(), "stderr: {:?}", completed.stderr);
    let inbox_output = inbox_wait.wait_with_output().unwrap();
    assert!(
        inbox_output.status.success(),
        "stderr: {:?}",
        inbox_output.stderr
    );
    assert!(String::from_utf8_lossy(&inbox_output.stdout).contains("native-child done rc=7"));

    let repeated = boop(
        &dir,
        &["beep", "agent", "done", "native-child", "--rc", "7"],
    );
    assert!(
        !repeated.status.success(),
        "completion must be explicit and once"
    );

    let text = std::fs::read_to_string(dir.join("bus.ndjson")).unwrap();
    let result_ids = text
        .lines()
        .filter(|line| {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            row["kind"] == "result" && row["from"] == "native-child"
        })
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(result_ids.len(), 1);
    assert!(
        !dir.join("registry.json").exists() || {
            let registry: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(dir.join("registry.json")).unwrap())
                    .unwrap();
            registry.get("native-child").is_none()
        }
    );
    let _ = std::fs::remove_dir_all(&dir);
}
