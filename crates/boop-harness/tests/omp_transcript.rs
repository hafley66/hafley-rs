//! omp transcript readers, through the real omp where it is installed. The
//! fixture-file legs need no omp; the live legs (real run, resume, incremental)
//! run omp against a working OpenRouter key and skip when omp is absent.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use boop_harness::harness::omp::Omp;
use boop_harness::{Harness, HarnessId};

/// Tests that rewrite `PI_CODING_AGENT_DIR` serialize here; the harness reads it.
static ENV_LOCK: Mutex<()> = Mutex::new(());

const FIXTURE: &str = "tests/fixtures/omp/session.jsonl";
const FIXTURE_ID: &str = "01a0a126-b356-7000-9e69-cc5fadd7991d";
const FIXTURE_CWD: &str =
    "/Users/chrishafley/projects/hafley-rs/.boop-worktrees/test/omp-harness-probe";
const FIXTURE_MODEL: &str = "deepseek/deepseek-v4-flash-0731";
const FIXTURE_INPUT_TOKENS: u64 = 18844;

/// The `omp` executable this machine would spawn, or `None` when it is absent.
fn omp_binary() -> Option<PathBuf> {
    boop_harness::harness::mock_tui::resolve_executable("omp", "OMP_BIN")
}

/// The working OpenRouter key from the caller's own opencode config; the env
/// `OPENROUTER_API_KEY` is expired.
fn openrouter_key() -> Option<String> {
    let home = dirs::home_dir().unwrap_or_default();
    let path = home.join(".config").join("opencode").join("opencode.json");
    let text = std::fs::read_to_string(&path).ok()?;
    let root = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    root
        .get("provider")
        .and_then(|provider| provider.get("openrouter"))
        .and_then(|openrouter| openrouter.get("options"))
        .and_then(|options| options.get("apiKey"))
        .and_then(serde_json::Value::as_str)
        .filter(|key| !key.is_empty())
        .map(str::to_owned)
}

/// Set `name` for the spawned child and restore it afterward.
fn with_env<T>(name: &str, value: Option<&str>, run: impl FnOnce() -> T) -> T {
    let saved = std::env::var_os(name);
    match value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
    let outcome = run();
    match saved {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
    outcome
}

/// Run the real omp to completion from `cwd` with `extra_args`, bounded to 120s.
fn run_omp(cwd: &Path, agent_dir: &Path, key: &str, extra_args: &[&str]) {
    use wait_timeout::ChildExt;
    let mut child = std::process::Command::new("omp")
        .current_dir(cwd)
        .env("PI_CODING_AGENT_DIR", agent_dir)
        .env("OPENROUTER_API_KEY", key)
        .arg("--allow-home")
        .arg("--model")
        .arg("deepseek/deepseek-v4-flash-0731")
        .args(extra_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn omp");
    let timed_out = child
        .wait_timeout(std::time::Duration::from_secs(120))
        .expect("wait on omp")
        .is_none();
    if timed_out {
        let _ = child.kill();
        panic!("omp timed out after 120s");
    }
    let status = child.wait().expect("reap omp");
    assert!(status.success(), "omp failed: {status}");
}

fn count_lines(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .expect("read transcript")
        .lines()
        .count()
}

/// Test 1: the fixture parses to one session whose header id and cwd survive,
/// and `read_from` decodes exactly the two message events with no skips. The
/// strip metadata carries the assistant model and the fixture's input tokens.
#[test]
fn fixture_parse_lists_one_session_and_reads_two_messages() {
    let dir = tempfile::tempdir().expect("temp agent dir");
    let encoded = dir.path().join("sessions").join("-x-");
    std::fs::create_dir_all(&encoded).expect("create encoded cwd dir");
    let dest = encoded.join(format!("2026-09-14T18-21-03-191Z_{FIXTURE_ID}.jsonl"));
    std::fs::copy(FIXTURE, &dest).expect("copy fixture transcript");

    let _guard = ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let sessions = with_env(
        "PI_CODING_AGENT_DIR",
        Some(dir.path().to_str().unwrap()),
        || Omp.sessions().expect("discover omp sessions"),
    );
    assert_eq!(sessions.len(), 1, "fixture is exactly one session");
    let session = &sessions[0];
    assert_eq!(session.session_id, FIXTURE_ID);
    assert_eq!(session.cwd.as_deref(), Some(FIXTURE_CWD));
    assert_eq!(session.harness, HarnessId::Omp);

    let chunk = Omp.read_from(session, 0).expect("read fixture");
    assert_eq!(chunk.events.len(), 2, "exactly user and assistant message events");
    assert_eq!(chunk.skipped, 0, "recognized non-events do not count as skipped");
    assert_eq!(
        chunk
            .events
            .iter()
            .filter(|event| event.record_type == "message")
            .count(),
        2
    );

    let meta = Omp.describe(session).expect("describe fixture");
    assert_eq!(meta.model.as_deref(), Some(FIXTURE_MODEL));
    assert_eq!(meta.input_tokens, Some(FIXTURE_INPUT_TOKENS));

    let messages = Omp.messages(session, None);
    assert_eq!(messages.len(), 2, "user and assistant turns");
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[1].role, "assistant");
}

/// Test 2: a real omp run writes a session whose transcript yields at least one
/// tool call and one tool result event, ingests into a store, and describes the
/// model. Skipped when omp is absent; reports whether it ran live.
#[test]
fn real_run_then_ingest() {
    let Some(_omp) = omp_binary() else {
        println!("omp_transcript real_run_then_ingest: SKIPPED (omp not on PATH)");
        return;
    };
    let Some(key) = openrouter_key() else {
        println!("omp_transcript real_run_then_ingest: SKIPPED (no openrouter key)");
        return;
    };
    let agent_dir = tempfile::tempdir().expect("temp agent dir");
    let cwd = tempfile::tempdir().expect("temp cwd");

    let _guard = ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    run_omp(
        cwd.path(),
        agent_dir.path(),
        &key,
        &["--print", "run the shell command: echo bench-ok"],
    );

    let sessions = with_env(
        "PI_CODING_AGENT_DIR",
        Some(agent_dir.path().to_str().unwrap()),
        || Omp.sessions().expect("discover omp sessions"),
    );
    assert_eq!(sessions.len(), 1, "one real run writes one session");
    let session = &sessions[0];

    let chunk = Omp.read_from(session, 0).expect("read real transcript");
    let tool_names: Vec<String> = chunk
        .events
        .iter()
        .filter_map(|event| event.tool_name.clone())
        .collect();
    assert!(
        chunk.events.iter().any(|event| event.record_type == "toolCall"),
        "a tool call event is present, tool names: {tool_names:?}"
    );
    assert!(
        chunk.events.iter().any(|event| event.record_type == "toolResult"),
        "a tool result event is present, tool names: {tool_names:?}"
    );
    println!(
        "omp_transcript real_run_then_ingest: toolCall shape record_type={:?} tool_name={:?}",
        chunk
            .events
            .iter()
            .find(|event| event.record_type == "toolCall")
            .map(|event| &event.record_type),
        tool_names.first(),
    );

    let db = std::env::temp_dir().join(format!(
        "boop_omp_transcript_ingest_{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&db);
    let store = boop_store::ident::Store::open(db.clone()).expect("open store");
    let ingested = Omp.ingest(&store, session, 0).expect("ingest real transcript");
    assert!(ingested.stat.written >= 2, "user, tool and assistant turns land");
    assert!(ingested.stat.usage_written >= 1, "the assistant usage row lands");
    drop(store);
    let totals = boop_store::testing::usage_totals_at(&db);
    assert!(totals.input_tokens > 0, "usage input tokens are non-zero");
    let _ = std::fs::remove_file(&db);

    let meta = Omp.describe(session).expect("describe real session");
    assert_eq!(meta.model.as_deref(), Some("deepseek/deepseek-v4-flash-0731"));
    println!("omp_transcript real_run_then_ingest: ran LIVE");
}

/// Tests 3 and 4: resume reuses the same file (no second session) and a
/// subsequent `read_from` from the earlier cursor returns only the new lines
/// with `reset == false`. Skipped when omp is absent.
#[test]
fn resume_reuses_the_same_file_and_reads_only_new_lines() {
    let Some(_omp) = omp_binary() else {
        println!("omp_transcript resume: SKIPPED (omp not on PATH)");
        return;
    };
    let Some(key) = openrouter_key() else {
        println!("omp_transcript resume: SKIPPED (no openrouter key)");
        return;
    };
    let agent_dir = tempfile::tempdir().expect("temp agent dir");
    let cwd = tempfile::tempdir().expect("temp cwd");

    let _guard = ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    run_omp(
        cwd.path(),
        agent_dir.path(),
        &key,
        &["--print", "run the shell command: echo bench-ok"],
    );

    let sessions = with_env(
        "PI_CODING_AGENT_DIR",
        Some(agent_dir.path().to_str().unwrap()),
        || Omp.sessions().expect("discover omp sessions"),
    );
    assert_eq!(sessions.len(), 1);
    let session = sessions[0].clone();
    let path = session.path.clone();
    let before = count_lines(&path);
    let cursor = Omp.read_from(&session, 0).expect("first read").next_offset;

    let uuid = Omp.resume_id(&session).to_owned();
    assert_eq!(uuid, session.session_id, "resume id is the header uuid");

    run_omp(
        cwd.path(),
        agent_dir.path(),
        &key,
        &["--session", &uuid, "--print", "--allow-home", "reply pong"],
    );

    let sessions = with_env(
        "PI_CODING_AGENT_DIR",
        Some(agent_dir.path().to_str().unwrap()),
        || Omp.sessions().expect("discover omp sessions after resume"),
    );
    assert_eq!(sessions.len(), 1, "resume appends to the same file, no second session");
    let after = count_lines(&path);
    assert!(after > before, "resume appended lines: before={before} after={after}");

    let chunk = Omp.read_from(&session, cursor).expect("incremental read");
    assert!(!chunk.reset, "file was not truncated");
    assert!(
        chunk.events.len() > 0,
        "the resumed turn yields new events after the first cursor"
    );
    assert_eq!(chunk.next_offset, std::fs::metadata(&path).unwrap().len());
    println!("omp_transcript resume: ran LIVE");
}