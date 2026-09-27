#![cfg(feature = "cli")]

use std::io::Write as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use crate::daemon_guard::DaemonGuard;

fn copy_fixture(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("fixture directory");
    for entry in std::fs::read_dir(source).expect("fixture entries") {
        let entry = entry.expect("fixture entry");
        if entry.file_name() == ".dl" {
            continue;
        }
        let dest = target.join(entry.file_name());
        if entry.file_type().expect("fixture type").is_dir() {
            copy_fixture(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).expect("copy fixture file");
        }
    }
}

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

fn run_stdin(
    binary: &Path,
    cache: Option<&Path>,
    cwd: Option<&Path>,
    args: &[&str],
    input: &[u8],
) -> Output {
    let mut command = Command::new(binary);
    command
        .args(args)
        .env("DL_TRAIL", "0")
        .env("RUST_LOG", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cache) = cache {
        command
            .env("XDG_CACHE_HOME", cache)
            .env("RYI_IDLE_SECS", "5");
    }
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command.spawn().expect("stdin process");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(input)
        .expect("write stdin bytes");
    child.wait_with_output().expect("stdin process output")
}

#[test]
fn direct_server_and_daemon_client_replacement_and_idle_exit() {
    let scratch = tempfile::tempdir().expect("round trip scratch");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                .join(".cache/lanes/shared/target")
        });
    let build = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args(["build", "-p", "ryi", "-j", "4"])
        .env("CARGO_TARGET_DIR", &target)
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
    let server = bin.join("ryii");
    std::fs::copy(target.join("debug/ryi"), &client).expect("copy client");
    std::fs::copy(env!("CARGO_BIN_EXE_ryii"), &server).expect("copy server");

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
    assert!(
        direct.status.success(),
        "direct: {}",
        String::from_utf8_lossy(&direct.stderr)
    );
    assert!(
        !resident.socket().exists(),
        "direct ryii does not start a daemon"
    );

    let plain = Command::new(&server)
        .args(["fast", file])
        .env("DL_TRAIL", "0")
        .env("RUST_LOG", "off")
        .output()
        .expect("direct absolute command");
    assert!(
        plain.status.success(),
        "direct: {}",
        String::from_utf8_lossy(&plain.stderr)
    );
    assert!(!resident.socket().exists(), "direct ryii skips the socket");

    let relative_cwd = workspace.join("crates/soopy");
    let relative_cache = scratch.path().join("relative-cache");
    let mut relative_daemon = DaemonGuard::new(&relative_cache);
    let direct_dot = Command::new(&server)
        .args(["fast", "."])
        .current_dir(&relative_cwd)
        .env("DL_TRAIL", "0")
        .env("RUST_LOG", "off")
        .output()
        .expect("direct relative cwd command");
    assert!(
        direct_dot.status.success(),
        "direct fast .: {}",
        String::from_utf8_lossy(&direct_dot.stderr)
    );
    assert!(
        !relative_daemon.socket().exists(),
        "direct fast . does not start a daemon"
    );
    let daemon_dot = Command::new(&client)
        .args(["fast", "."])
        .current_dir(&relative_cwd)
        .env("XDG_CACHE_HOME", &relative_cache)
        .env("RYI_IDLE_SECS", "5")
        .env("DL_TRAIL", "0")
        .env("RUST_LOG", "off")
        .output()
        .expect("daemon relative cwd command");
    assert_eq!(
        daemon_dot.status.code(),
        direct_dot.status.code(),
        "fast . exit code"
    );
    let first_difference = daemon_dot
        .stdout
        .iter()
        .zip(&direct_dot.stdout)
        .position(|(daemon, direct)| daemon != direct);
    let counts = |bytes: &[u8]| {
        let mut counts = std::collections::BTreeMap::<String, usize>::new();
        for line in bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
        {
            let row: serde_json::Value = serde_json::from_slice(line).expect("fast JSONL row");
            *counts
                .entry(row["record"].as_str().unwrap_or("?").to_string())
                .or_default() += 1;
        }
        counts
    };
    assert!(daemon_dot.stdout == direct_dot.stdout,
        "fast . stdout from the same cwd: daemon {} bytes, direct {} bytes, first difference {first_difference:?}, daemon counts {:?}, direct counts {:?}",
        daemon_dot.stdout.len(), direct_dot.stdout.len(), counts(&daemon_dot.stdout), counts(&direct_dot.stdout));
    let relative_path_list = b"src/lib.rs\n";
    let direct_stdin = run_stdin(
        &server,
        None,
        Some(&relative_cwd),
        &["fast", "-"],
        relative_path_list,
    );
    let daemon_stdin = run_stdin(
        &client,
        Some(&relative_cache),
        Some(&relative_cwd),
        &["fast", "-"],
        relative_path_list,
    );
    assert_eq!(
        daemon_stdin.status.code(),
        direct_stdin.status.code(),
        "fast - relative path-list exit"
    );
    assert_eq!(
        daemon_stdin.stdout, direct_stdin.stdout,
        "fast - relative path-list stdout"
    );

    let fixture_cwd = scratch.path().join("type_ladder");
    copy_fixture(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/type_ladder"),
        &fixture_cwd,
    );
    for (cwd, args) in [
        (relative_cwd.as_path(), vec!["fast", "does/not/exist.rs"]),
        (fixture_cwd.as_path(), vec!["slow", "."]),
        (
            fixture_cwd.as_path(),
            vec![
                "query",
                "--query",
                "(struct_item name: (type_identifier) @n)",
                "src",
            ],
        ),
    ] {
        let direct = Command::new(&server)
            .args(&args)
            .current_dir(cwd)
            .env("DL_TRAIL", "0")
            .env("RUST_LOG", "off")
            .output()
            .expect("direct relative command");
        let daemon = Command::new(&client)
            .args(&args)
            .current_dir(cwd)
            .env("XDG_CACHE_HOME", &relative_cache)
            .env("RYI_IDLE_SECS", "5")
            .env("DL_TRAIL", "0")
            .env("RUST_LOG", "off")
            .output()
            .expect("daemon relative command");
        assert_eq!(
            daemon.status.code(),
            direct.status.code(),
            "{args:?} exit code"
        );
        assert_eq!(daemon.stdout, direct.stdout, "{args:?} stdout");
    }
    assert!(relative_daemon.stop(), "relative daemon exited");

    let trace = scratch.path().join("daemon-observe.json");
    let first = run(&client, &cache, &["fast", file], None, Some(&trace));
    assert!(
        first.status.success(),
        "daemon: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(resident.socket().exists(), "ryi starts the daemon");
    assert_eq!(first.stdout, plain.stdout, "direct and daemon stdout");
    let second = run(&client, &cache, &["fast", file], None, Some(&trace));
    assert_eq!(
        second.stdout, first.stdout,
        "second request on resident daemon"
    );

    let path_list = format!("{file}\n");
    let direct_paths = run_stdin(&server, None, None, &["fast", "-"], path_list.as_bytes());
    let daemon_paths = run_stdin(
        &client,
        Some(&cache),
        None,
        &["fast", "-"],
        path_list.as_bytes(),
    );
    assert_eq!(
        daemon_paths.status.code(),
        direct_paths.status.code(),
        "raw path-list exit"
    );
    assert_eq!(
        daemon_paths.stdout, direct_paths.stdout,
        "raw path-list stdout"
    );

    let region = scratch.path().join("region.rs");
    std::fs::write(&region, b"// sprefa:auto-begin demo\nraw generated text\nwith another line\n// sprefa:auto-end demo\n").unwrap();
    let region = region.to_str().expect("UTF-8 region path");
    let generated = b"raw generated text\nwith another line\n";
    let region_args = ["region", region, "demo", "--generated", "-"];
    let direct_region = run_stdin(&server, None, None, &region_args, generated);
    let daemon_region = run_stdin(&client, Some(&cache), None, &region_args, generated);
    assert_eq!(
        daemon_region.status.code(),
        direct_region.status.code(),
        "raw region exit"
    );
    assert_eq!(
        daemon_region.stdout, direct_region.stdout,
        "raw region stdout"
    );

    let ingest_bytes = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tsi/foreign_probe.jsonl"),
    )
    .expect("TSI fixture bytes");
    let direct_ingest = run_stdin(
        &server,
        None,
        None,
        &["ingest", "/dev/stdin"],
        &ingest_bytes,
    );
    let daemon_ingest = run_stdin(
        &client,
        Some(&cache),
        None,
        &["ingest", "/dev/stdin"],
        &ingest_bytes,
    );
    assert_eq!(
        daemon_ingest.status.code(),
        direct_ingest.status.code(),
        "raw ingest exit"
    );
    assert_eq!(
        daemon_ingest.stdout, direct_ingest.stdout,
        "raw ingest stdout"
    );
    let unknown = scratch.path().join("unknown.extension");
    std::fs::write(&unknown, b"unrecognized source\n").expect("unknown fixture");
    let unknown = unknown.to_str().expect("UTF-8 unknown path");
    let plain_diagnostic = Command::new(&server)
        .arg(unknown)
        .env("DL_TRAIL", "0")
        .env("RUST_LOG", "off")
        .output()
        .expect("direct diagnostic");
    let daemon_diagnostic = run(&client, &cache, &[unknown], None, None);
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
    let replaced = run(&client, &cache, &["fast", file], None, None);
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
        3,
        "three fast request spans in observe sink: {:?}",
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
    let output = run(&client, &idle_cache, &["fast", file], Some(1), None);
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
                    run(client, race_cache, &["fast", file], None, None)
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
    let start = Command::new(env!("CARGO_BIN_EXE_ryii"))
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
