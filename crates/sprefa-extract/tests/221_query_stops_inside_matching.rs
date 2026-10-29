//! A 3-sibling pattern over a 100-key object literal spends its time inside one
//! tree-sitter match; `--timeout` and a disconnected daemon client both stop it there.
//! The `*` on the middle child keeps scm++ from splitting the siblings.
use std::io::Write as _;
use std::path::Path;
use std::time::Duration;

use crate::daemon_guard::DaemonGuard;

/// `(dir, file, pattern)`: the object literal and the scm++ program.
fn siblings(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let file = dir.join("object.ts");
    let keys: String = (0..100).map(|key| format!("  k{key}: {key},\n")).collect();
    std::fs::write(&file, format!("export const big = {{\n{keys}}};\n")).unwrap();
    let pattern = dir.join("three.scm");
    std::fs::write(&pattern, "(object (pair) @a (pair)* @b (pair) @c)\n").unwrap();
    (file, pattern)
}

#[test]
fn query_timeout_stops_inside_one_match() {
    let dir = tempfile::tempdir().unwrap();
    let (file, pattern) = siblings(dir.path());
    // Stop latency against input size is `growth_bench_timeout_latency_is_flat`.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
        .args(["query", "--scmpp"])
        .arg(&pattern)
        .args(["--timeout", "3"])
        .arg(&file)
        .output()
        .unwrap();
    insta::assert_snapshot!(
        format!("exit={:?}\n{}", output.status.code(), String::from_utf8_lossy(&output.stderr).trim()),
        @r"
    exit=Some(3)
    query (typescript): scm++ level 0: `(object ...)` has 3 unanchored sibling children; tree-sitter tries every ordered pick of them, so its cost grows combinatorially in the matching children
    query: query exceeded 3s
    "
    );
}

/// CPU seconds `ps` reports for `pid`, `M:SS.ss`.
fn cpu_seconds(pid: i32) -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "time=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    let (minutes, seconds) = text.trim().split_once(':').unwrap();
    minutes.parse::<f64>().unwrap() * 60.0 + seconds.parse::<f64>().unwrap()
}

#[test]
fn daemon_query_stops_when_its_client_disconnects() {
    let dir = tempfile::tempdir().unwrap();
    let (file, pattern) = siblings(dir.path());
    let cache = dir.path().join("cache");
    let mut daemon = DaemonGuard::new(&cache);
    let spawned = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
        .arg("--daemon")
        .env("XDG_CACHE_HOME", &cache)
        .env("RYI_IDLE_SECS", "60")
        .status()
        .unwrap();
    assert!(spawned.success());
    for _ in 0..400 {
        if daemon.pid().is_some() && daemon.socket().exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let pid = daemon.pid().expect("daemon wrote its pid");
    let body = serde_json::json!({
        "request_root": dir.path(),
        "args": {"paths": [file], "scmpp": pattern},
    })
    .to_string();
    let mut client = std::os::unix::net::UnixStream::connect(daemon.socket()).unwrap();
    write!(
        client,
        "POST /query HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    std::thread::sleep(Duration::from_secs(1));
    let busy = cpu_seconds(pid);
    drop(client);
    // The timer polls the flag every 100 ms; give the stop and its unwinding a second.
    std::thread::sleep(Duration::from_secs(1));
    let after_stop = cpu_seconds(pid);
    std::thread::sleep(Duration::from_secs(2));
    let idle = cpu_seconds(pid) - after_stop;
    assert!(busy > 0.5, "the query ran on the daemon before the disconnect: {busy}s CPU");
    assert!(idle < 0.3, "daemon kept matching after its client left: {idle}s CPU in 2s");
    assert!(daemon.stop());
}
