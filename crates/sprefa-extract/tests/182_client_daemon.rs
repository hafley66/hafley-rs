#![cfg(feature = "cli")]

use std::os::unix::fs::MetadataExt as _;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use crate::daemon_guard::DaemonGuard;

fn run(
    client: &Path,
    cache: &Path,
    args: &[&str],
    idle: Option<u64>,
    trace: Option<&Path>,
) -> Output {
    let mut command = Command::new(client);
    command
        .args(args)
        .env("XDG_CACHE_HOME", cache)
        .env("DL_TRAIL", "0")
        .env("RUST_LOG", "off")
        .env("RYI_IDLE_SECS", idle.unwrap_or(5).to_string());
    if let Some(path) = trace {
        command.env("HAFLEY_TRACE", path);
    }
    command.output().expect("client process")
}

#[test]
fn plain_and_daemon_replacement_and_idle_exit() {
    let scratch = tempfile::tempdir().expect("round trip scratch");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = scratch.path().join("client-target");
    let build = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args(["build", "-p", "ryi", "--target-dir"])
        .arg(&target)
        .current_dir(&workspace)
        .output()
        .expect("build thin client");
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );

    let bin = scratch.path().join("bin");
    std::fs::create_dir_all(&bin).expect("binary directory");
    let client = bin.join("ryi");
    let server = bin.join("ryi-server");
    std::fs::copy(target.join("debug/ryi"), &client).expect("copy client");
    std::fs::copy(env!("CARGO_BIN_EXE_ryi-server"), &server).expect("copy server");

    let file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/type_ladder/src/_0_types.rs");
    let file = file.canonicalize().expect("absolute fixture");
    let file = file.to_str().expect("UTF-8 fixture path");
    let cache = scratch.path().join("cache");
    let mut resident = DaemonGuard::new(&cache);

    let relative = "tests/fixtures/type_ladder/src";
    let direct = Command::new(&server)
        .args(["fast", relative])
        .env("DL_TRAIL", "0")
        .env("RUST_LOG", "off")
        .output()
        .expect("direct relative command");
    let relative_client = run(&client, &cache, &["fast", relative], None, None);
    assert_eq!(
        relative_client.status.code(),
        direct.status.code(),
        "relative exit code"
    );
    assert_eq!(
        relative_client.stdout, direct.stdout,
        "relative stdout bytes"
    );
    assert_eq!(
        relative_client.stderr, direct.stderr,
        "relative stderr bytes"
    );
    assert!(
        !resident.socket().exists(),
        "plain ryi does not start a daemon"
    );

    let plain = run(&client, &cache, &["fast", file], None, None);
    assert!(
        plain.status.success(),
        "plain: {}",
        String::from_utf8_lossy(&plain.stderr)
    );
    assert!(
        !resident.socket().exists(),
        "plain command skips the socket"
    );

    let trace = scratch.path().join("daemon-observe.json");
    let first = run(
        &client,
        &cache,
        &["--daemon-client", "fast", file],
        None,
        Some(&trace),
    );
    assert!(
        first.status.success(),
        "daemon: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, plain.stdout, "plain and daemon stdout");
    let second = run(
        &client,
        &cache,
        &["--daemon-client", "fast", file],
        None,
        Some(&trace),
    );
    assert_eq!(
        second.stdout, first.stdout,
        "second request on resident daemon"
    );
    let unknown = scratch.path().join("unknown.extension");
    std::fs::write(&unknown, b"unrecognized source\n").expect("unknown fixture");
    let unknown = unknown.to_str().expect("UTF-8 unknown path");
    let plain_diagnostic = run(&client, &cache, &[unknown], None, None);
    let daemon_diagnostic = run(&client, &cache, &["--daemon-client", unknown], None, None);
    assert_eq!(daemon_diagnostic.stdout, plain_diagnostic.stdout);
    assert_eq!(daemon_diagnostic.stderr, plain_diagnostic.stderr);
    assert!(String::from_utf8_lossy(&daemon_diagnostic.stderr).contains("0 facts:"));

    let first_query = scratch.path().join("first.rs");
    let unsupported_query = scratch.path().join("second.txt");
    std::fs::write(&first_query, "fn first() {}\n").unwrap();
    std::fs::write(&unsupported_query, "second\n").unwrap();
    let late = run(
        &client,
        &cache,
        &[
            "--daemon-client",
            "query",
            first_query.to_str().unwrap(),
            unsupported_query.to_str().unwrap(),
            "--query",
            "(function_item name: (identifier) @name)",
        ],
        None,
        None,
    );
    let formatted = Command::new(&server)
        .args([
            "--format",
            "jsonl",
            "query",
            first_query.to_str().unwrap(),
            unsupported_query.to_str().unwrap(),
            "--query",
            "(function_item name: (identifier) @name)",
        ])
        .env("DL_TRAIL", "0")
        .env("RUST_LOG", "off")
        .output()
        .expect("in-process JSONL query");
    assert_eq!(
        late.status.code(),
        Some(2),
        "late stream error uses the exit trailer"
    );
    assert_eq!(
        late.status.code(),
        formatted.status.code(),
        "formatted in-process exit code"
    );
    assert_eq!(
        late.stdout, formatted.stdout,
        "formatted in-process stdout includes the final error row"
    );
    let late_rows: Vec<serde_json::Value> = late
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).expect("JSONL response row"))
        .collect();
    assert_eq!(late_rows.len(), 2);
    assert_eq!(late_rows[0]["name"], "first");
    assert_eq!(late_rows[1]["code"], 2);
    let old_inode = std::fs::metadata(resident.socket())
        .expect("bound socket")
        .ino();

    let opened = std::fs::OpenOptions::new()
        .write(true)
        .open(&server)
        .expect("server binary");
    let modified = opened
        .metadata()
        .expect("server metadata")
        .modified()
        .expect("mtime");
    opened
        .set_times(std::fs::FileTimes::new().set_modified(modified + Duration::from_secs(2)))
        .expect("change server identity");
    drop(opened);
    let replaced = run(
        &client,
        &cache,
        &["--daemon-client", "fast", file],
        None,
        None,
    );
    assert!(
        replaced.status.success(),
        "replacement: {}",
        String::from_utf8_lossy(&replaced.stderr)
    );
    assert_eq!(replaced.stdout, plain.stdout, "replacement stdout");
    assert_ne!(
        std::fs::metadata(resident.socket())
            .expect("new socket")
            .ino(),
        old_inode
    );
    let timeline: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&trace).expect("daemon observe sink"))
            .expect("closed daemon timeline");
    let requests: std::collections::HashSet<_> = timeline
        .as_array()
        .expect("chrome events")
        .iter()
        .filter(|event| {
            event["name"] == "daemon_request"
                && event["ph"] == "E"
                && event["args"]["verb"] == "fast"
                && event["args"]["request_root"]
                    == std::env::current_dir().unwrap().to_string_lossy().as_ref()
        })
        .filter_map(|event| {
            event["args"]["request_id"]
                .as_u64()
                .or_else(|| event["args"]["request_id"].as_str()?.parse().ok())
        })
        .collect();
    assert_eq!(
        requests.len(),
        2,
        "two fast request spans in observe sink: {:?}",
        timeline
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["name"] == "daemon_request")
            .take(8)
            .collect::<Vec<_>>()
    );

    let idle_cache = scratch.path().join("idle-cache");
    let mut idle = DaemonGuard::new(&idle_cache);
    let output = run(
        &client,
        &idle_cache,
        &["--daemon-client", "fast", file],
        Some(1),
        None,
    );
    assert!(
        output.status.success(),
        "idle request: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, plain.stdout, "idle daemon stdout");
    assert!(idle.socket().exists(), "idle socket started");
    let deadline = Instant::now() + Duration::from_secs(10);
    while idle.socket().exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!idle.socket().exists(), "idle daemon removed its socket");

    let race_cache = scratch.path().join("race-cache");
    let mut race = DaemonGuard::new(&race_cache);
    let barrier = std::sync::Barrier::new(3);
    let race_outputs = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let barrier = &barrier;
                let client = &client;
                let race_cache = &race_cache;
                scope.spawn(move || {
                    barrier.wait();
                    run(
                        client,
                        race_cache,
                        &["--daemon-client", "fast", file],
                        None,
                        None,
                    )
                })
            })
            .collect();
        barrier.wait();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    for output in race_outputs {
        assert!(
            output.status.success(),
            "racing client: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, plain.stdout);
    }
    assert!(
        race.socket().exists(),
        "one daemon socket serves both clients"
    );
    assert!(resident.stop(), "resident daemon exited");
    assert!(idle.stop(), "idle daemon exited");
    assert!(race.stop(), "racing daemon exited");
    for cache in [&cache, &idle_cache, &race_cache] {
        let guard = DaemonGuard::new(cache);
        assert!(
            !guard.socket().exists(),
            "no daemon socket survives its test cache"
        );
        assert!(
            guard.pid().is_none(),
            "no daemon PID survives its test cache"
        );
    }
}

#[test]
fn long_cache_socket_is_short_and_test_daemon_exits() {
    let scratch = tempfile::tempdir().expect("socket scratch");
    let cache = scratch.path().join("nested").join("x".repeat(120));
    let mut daemon = DaemonGuard::new(&cache);
    assert!(
        daemon.socket().as_os_str().len() <= 100,
        "socket fits unix path limit"
    );
    let start = Command::new(env!("CARGO_BIN_EXE_ryi-server"))
        .arg("--daemon")
        .env("XDG_CACHE_HOME", &cache)
        .env("RYI_IDLE_SECS", "5")
        .status()
        .expect("start daemon");
    assert!(start.success(), "daemon start");
    let until = Instant::now() + Duration::from_secs(5);
    while daemon.pid().is_none() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(25));
    }
    let pid = daemon.pid().expect("daemon PID recorded under long cache");
    assert!(
        DaemonGuard::alive(pid),
        "daemon running before guard cleanup"
    );
    assert!(daemon.stop(), "daemon stopped and process exited");
    assert!(
        !DaemonGuard::alive(pid),
        "no daemon survives under this XDG_CACHE_HOME"
    );
    assert!(!daemon.socket().exists(), "socket removed after cleanup");
}
