use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use boop::Store;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");
const STORED_SESSIONS: usize = 4_136;
const WRAPPER_PROCESSES: usize = 6;
const IDLE_PASSES_PER_PROCESS: usize = 4;

/// libtest names a test by its module path with the crate root dropped. This
/// file is a module of `tests/main.rs`, so a re-entry filter written as a bare
/// function name matches nothing and the child silently runs zero tests.
fn test_name(function: &str) -> String {
    match module_path!().split_once("::") {
        Some((_crate_root, module)) => format!("{module}::{function}"),
        None => function.to_owned(),
    }
}

struct Fixture {
    root: PathBuf,
    children: Vec<Child>,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "boop-native-projector-contention-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Self {
            root,
            children: Vec::new(),
        }
    }

    fn db(&self) -> PathBuf {
        self.root.join("boop.db")
    }

    fn trail(&self) -> PathBuf {
        self.root.join("known-sessions-%p.trail")
    }

    fn start(&self) -> PathBuf {
        self.root.join("start")
    }

    fn ready(&self) -> PathBuf {
        self.root.join("ready")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn write_transcript(home: &Path, index: usize) {
    let session = format!("{index:08}-0000-0000-0000-000000000000");
    let dir = home
        .join(".claude/projects")
        .join(format!("fixture-{index}"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{session}.jsonl")),
        format!(
            "{{\"type\":\"user\",\"uuid\":\"{session}-u\",\"sessionId\":\"{session}\",\"timestamp\":\"2026-08-24T12:00:00.000Z\",\"cwd\":\"/fixture\",\"message\":{{\"role\":\"user\",\"content\":\"seed\"}}}}\n"
        ),
    )
    .unwrap();
}

fn pass_counts(root: &Path) -> BTreeMap<u32, usize> {
    let mut counts = BTreeMap::new();
    for entry in std::fs::read_dir(root).unwrap().flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(pid) = name
            .strip_prefix("known-sessions-")
            .and_then(|name| name.strip_suffix(".trail"))
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        counts.insert(
            pid,
            std::fs::read_to_string(entry.path())
                .unwrap_or_default()
                .lines()
                .count(),
        );
    }
    counts
}

fn clear_pass_counts(root: &Path) {
    for entry in std::fs::read_dir(root).unwrap().flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("known-sessions-")
        {
            std::fs::remove_file(entry.path()).unwrap();
        }
    }
}

#[test]
fn projector_worker() {
    let Ok(db) = std::env::var("BOOP_CONTENTION_WORKER_DB") else {
        return;
    };
    let start = PathBuf::from(std::env::var_os("BOOP_CONTENTION_START").unwrap());
    hafley_observe::init(hafley_observe::Config {
        service_name: "boop-contention-worker",
        service_version: env!("CARGO_PKG_VERSION"),
        default_filter: "boop_store=debug",
        format: hafley_observe::OutputFormat::Json,
        ansi: false,
    })
    .unwrap();
    let store = Store::open(PathBuf::from(db)).unwrap();
    let known = store.known_sessions().unwrap();
    assert_eq!(known.len(), STORED_SESSIONS);
    let ready = PathBuf::from(std::env::var_os("BOOP_CONTENTION_READY").unwrap());
    std::fs::create_dir_all(&ready).unwrap();
    std::fs::write(ready.join(std::process::id().to_string()), "").unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    while !start.exists() {
        assert!(Instant::now() < deadline, "worker start barrier timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    for _ in 0..IDLE_PASSES_PER_PROCESS {
        assert_eq!(known.len(), STORED_SESSIONS);
    }
}

#[test]
fn six_idle_projector_processes_do_not_repeat_global_session_materialization() {
    let mut fixture = Fixture::new();
    for index in 0..STORED_SESSIONS {
        write_transcript(&fixture.root, index);
    }

    let seeded = Command::new(BOOP)
        .args(["db", "sync", "create"])
        .env("HOME", &fixture.root)
        .env("BOOP_DB", fixture.db())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(
        seeded.status.success(),
        "fixture sync failed: {}",
        String::from_utf8_lossy(&seeded.stderr)
    );
    let stored = Store::open_readonly(fixture.db())
        .unwrap()
        .query_sync_cursors(None)
        .unwrap()
        .len();
    assert_eq!(stored, STORED_SESSIONS);

    let version = Command::new(BOOP).arg("--version").output().unwrap();
    let version = String::from_utf8(version.stdout).unwrap().trim().to_owned();
    assert!(version.starts_with("boop 0.0.4 "), "{version}");
    let test_binary = std::env::current_exe().unwrap();
    for _ in 0..WRAPPER_PROCESSES {
        let child = Command::new(&test_binary)
            .args(["--exact", &test_name("projector_worker"), "--nocapture"])
            .env("BOOP_CONTENTION_WORKER_DB", fixture.db())
            .env("BOOP_CONTENTION_START", fixture.start())
            .env("BOOP_CONTENTION_READY", fixture.ready())
            .env("BOOP_KNOWN_SESSIONS_TRAIL", fixture.trail())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        fixture.children.push(child);
    }

    let ready_deadline = Instant::now() + Duration::from_secs(10);
    while std::fs::read_dir(fixture.ready())
        .map(|entries| entries.count())
        .unwrap_or(0)
        < WRAPPER_PROCESSES
    {
        assert!(
            Instant::now() < ready_deadline,
            "wrapper startup barrier timed out"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let startup_materializations = pass_counts(&fixture.root);
    assert_eq!(startup_materializations.len(), WRAPPER_PROCESSES);
    assert!(startup_materializations.values().all(|count| *count == 1));
    clear_pass_counts(&fixture.root);
    std::fs::write(fixture.start(), "").unwrap();

    let mut failures = Vec::new();
    let mut worker_logs = Vec::new();
    for child in fixture.children.drain(..) {
        let output = child.wait_with_output().unwrap();
        worker_logs.push(String::from_utf8_lossy(&output.stderr).into_owned());
        if !output.status.success() {
            failures.push(String::from_utf8_lossy(&output.stderr).into_owned());
        }
    }
    assert!(failures.is_empty(), "workers failed: {failures:#?}");
    let materialization_events = worker_logs
        .iter()
        .map(|log| log.matches("known sessions materialized").count())
        .sum::<usize>();
    let materialized_row_fields = worker_logs
        .iter()
        .map(|log| log.matches("\"rows.read\":4136").count())
        .sum::<usize>();
    assert_eq!(materialization_events, WRAPPER_PROCESSES);
    assert_eq!(materialized_row_fields, WRAPPER_PROCESSES * 3);

    let per_process = pass_counts(&fixture.root);
    let idle_materializations: usize = per_process.values().sum();
    assert_eq!(
        idle_materializations, 0,
        "boop={version} harnesses={{claude: {WRAPPER_PROCESSES}}} processes={WRAPPER_PROCESSES} stored_sessions={STORED_SESSIONS} startup_materializations={startup_materializations:?} idle_passes_per_process={IDLE_PASSES_PER_PROCESS} idle_global_materializations={idle_materializations} per_process={per_process:?} worker_logs={worker_logs:#?}"
    );
}
