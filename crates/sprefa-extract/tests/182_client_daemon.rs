#![cfg(feature = "cli")]

use std::io::Write as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

struct DaemonSocket(PathBuf);

impl Drop for DaemonSocket {
    fn drop(&mut self) {
        if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(&self.0) {
            let _ = stream.write_all(b"POST /__shutdown HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n");
        }
    }
}

fn run(client: &Path, cache: &Path, args: &[&str], idle: Option<u64>) -> Output {
    let mut command = Command::new(client);
    command.args(args).env("XDG_CACHE_HOME", cache).env("DL_TRAIL", "0").env("RUST_LOG", "off");
    if let Some(secs) = idle { command.env("RYI_IDLE_SECS", secs.to_string()); }
    command.output().expect("client process")
}

fn socket(cache: &Path) -> PathBuf { cache.join("ryi/ryi.sock") }

#[test]
fn fresh_daemon_replacement_and_idle_exit() {
    let scratch = tempfile::tempdir().expect("round trip scratch");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = scratch.path().join("client-target");
    let build = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args(["build", "-p", "ryi", "--target-dir"])
        .arg(&target)
        .current_dir(&workspace)
        .output()
        .expect("build thin client");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));

    let bin = scratch.path().join("bin");
    std::fs::create_dir_all(&bin).expect("binary directory");
    let client = bin.join("ryi");
    let server = bin.join("ryi-server");
    std::fs::copy(target.join("debug/ryi"), &client).expect("copy client");
    std::fs::copy(env!("CARGO_BIN_EXE_ryi-server"), &server).expect("copy server");

    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/type_ladder/src/_0_types.rs");
    let file = file.canonicalize().expect("absolute fixture");
    let file = file.to_str().expect("UTF-8 fixture path");
    let cache = scratch.path().join("cache");
    let resident = DaemonSocket(socket(&cache));

    let fresh = run(&client, &cache, &["--fresh", "fast", file], None);
    assert!(fresh.status.success(), "fresh: {}", String::from_utf8_lossy(&fresh.stderr));
    assert!(!resident.0.exists(), "fresh skips the socket");

    let first = run(&client, &cache, &["fast", file], None);
    assert!(first.status.success(), "daemon: {}", String::from_utf8_lossy(&first.stderr));
    assert_eq!(first.stdout, fresh.stdout, "fresh and daemon stdout");
    let old_inode = std::fs::metadata(&resident.0).expect("bound socket").ino();

    let opened = std::fs::OpenOptions::new().write(true).open(&server).expect("server binary");
    let modified = opened.metadata().expect("server metadata").modified().expect("mtime");
    opened.set_times(std::fs::FileTimes::new().set_modified(modified + Duration::from_secs(2)))
        .expect("change server identity");
    drop(opened);
    let replaced = run(&client, &cache, &["fast", file], None);
    assert!(replaced.status.success(), "replacement: {}", String::from_utf8_lossy(&replaced.stderr));
    assert_eq!(replaced.stdout, fresh.stdout, "replacement stdout");
    assert_ne!(std::fs::metadata(&resident.0).expect("new socket").ino(), old_inode);

    let idle_cache = scratch.path().join("idle-cache");
    let idle = DaemonSocket(socket(&idle_cache));
    let output = run(&client, &idle_cache, &["fast", file], Some(1));
    assert!(output.status.success(), "idle request: {}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, fresh.stdout, "idle daemon stdout");
    assert!(idle.0.exists(), "idle socket started");
    let deadline = Instant::now() + Duration::from_secs(10);
    while idle.0.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!idle.0.exists(), "idle daemon removed its socket");
}
