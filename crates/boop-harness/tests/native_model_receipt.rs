//! Repeatable terminal receipt for native Codex per-turn model attribution.
//!
//! The scrubbed rollout fixture is cut into chunks carrying an authored logical
//! schedule from zero, replayed through a real OS pipe into a file, then decoded
//! by the real `Codex` adapter into a real store. The assertion is the model the
//! `agent_usage` rows join through `dict_model`.
//!
//! The test writes only to the temp dir. It records machine evidence to the path
//! named by `BOOP_F41_EVIDENCE` when that variable is set, and reads source
//! identity from `BOOP_SOURCE_SHA`. An ordinary `cargo test` therefore never
//! touches a tracked file.
//!
//! On the base revision the assertion fails (`unknown`); after the fix it passes
//! (`gpt-5.6-luna`). The labelled receipt prints either way, then the test
//! asserts.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use boop_harness::harness::codex::Codex;
use boop_harness::harness::replay::{drive, parse_cast};
use boop_harness::{Harness, HarnessId, SessionRef};
use boop_store::Store;

const FIXTURE: &str = include_str!("fixtures/transcripts/codex/codex-native-model.jsonl");
const EXPECTED_MODEL: &str = "gpt-5.6-luna";

/// The fixture cut into three contiguous byte chunks, each carrying an authored
/// logical timestamp from zero. Chunk boundaries are transcript line
/// boundaries, so `read_complete_lines` never sees a partial line. `drive` does
/// not wall-clock pace these times; they are the recorded schedule, not elapsed
/// wall time.
fn timed_chunks(fixture: &str) -> Vec<(u64, Vec<u8>)> {
    let lines: Vec<&str> = fixture.split_inclusive('\n').collect();
    let cuts = [3.min(lines.len()), 5.min(lines.len()), lines.len()];
    let mut chunks = Vec::new();
    let mut start = 0;
    for (index, cut) in cuts.iter().enumerate() {
        let chunk = lines[start..*cut].concat();
        chunks.push((index as u64 * 50, chunk.into_bytes()));
        start = *cut;
    }
    chunks
}

fn pipe_replay_to_file(chunks: &[(u64, Vec<u8>)], target: &Path) -> i32 {
    let header = serde_json::json!({"version": 2, "width": 80, "height": 24});
    let mut cast = serde_json::to_string(&header).unwrap();
    for (at_ms, bytes) in chunks {
        let text = std::str::from_utf8(bytes).expect("fixture is utf-8");
        let seconds = *at_ms as f64 / 1000.0;
        cast.push('\n');
        cast.push_str(&serde_json::to_string(&serde_json::json!([seconds, "i", text])).unwrap());
    }
    cast.push('\n');
    cast.push_str(&serde_json::to_string(&serde_json::json!([0.5, "m", "replay-eof"])).unwrap());

    let schedule = parse_cast(&cast).expect("parse transcript schedule");
    assert_eq!(
        schedule.first().map(|event| event.at_ms),
        Some(0),
        "the receipt's relative clock starts at zero"
    );

    let mut child = Command::new("sh")
        .args(["-c", "cat > \"$1\"", "sh"])
        .arg(target)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn cat child");
    let mut child_stdin = Some(child.stdin.take().expect("child stdin pipe"));
    let mut child_stdout = child.stdout.take().expect("child stdout pipe");
    let outcome = drive(&schedule, &mut child_stdin, &mut child_stdout, "replay-eof")
        .expect("drive transcript through pipe");
    let exit_code = child.wait().expect("cat exits").code().unwrap_or(-1);
    assert!(outcome.input_closed);
    exit_code
}

fn session(path: &Path) -> SessionRef {
    SessionRef {
        harness: HarnessId::Codex,
        session_id: "ses-native-model".into(),
        nickname: "Schrodinger".into(),
        path: path.to_path_buf(),
        cwd: Some("/scrubbed/workspace".into()),
        git_branch: None,
        modified_ms: 0,
        size: 0,
        tmux: None,
        tmux_socket: None,
        parent: Some("01a07c48-4ca3-75c3-9fdb-17c4c4ca1c58".into()),
    }
}

fn usage_models(store: &Store, session: &str) -> Vec<(String, i64)> {
    let filter = boop_store::usage::UsageQuery {
        session: Some(session.to_owned()),
        ..Default::default()
    };
    store
        .usage_report_rows(Some(boop_store::usage::GroupBy::Model), &filter)
        .unwrap()
        .into_iter()
        .map(|row| (row.bucket.unwrap_or_default(), row.calls))
        .collect()
}

#[test]
fn native_model_pipe_replay_receipt() {
    let chunks = timed_chunks(FIXTURE);
    let authored_schedule_ms: Vec<u64> = chunks.iter().map(|(at_ms, _)| *at_ms).collect();

    let transcript = std::env::temp_dir().join(format!(
        "boop_native_model_receipt_{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&transcript);
    let replay_start = Instant::now();
    let replay_child_exit = pipe_replay_to_file(&chunks, &transcript);
    let measured_pipe_elapsed_ms = replay_start.elapsed().as_millis();

    let replayed = std::fs::read(&transcript).expect("read replayed transcript");
    let recorded_equals_replayed = replayed == FIXTURE.as_bytes();
    assert!(recorded_equals_replayed, "recorded bytes survive the pipe");

    let db_path = std::env::temp_dir().join(format!(
        "boop_native_model_receipt_{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&db_path);
    let store = Store::open(db_path.clone()).expect("open store");
    let session = session(&transcript);
    Codex.ingest(&store, &session, 0).expect("ingest");
    let actual = usage_models(&store, &session.session_id);
    drop(store);

    let passed = actual == vec![(EXPECTED_MODEL.to_owned(), 2)];
    let label = if passed { "GREEN FIXED" } else { "RED  REPRO" };
    let ansi = if passed {
        "\x1b[32mGREEN FIXED\x1b[0m"
    } else {
        "\x1b[31mRED  REPRO\x1b[0m"
    };
    println!(
        "\n{ansi} | f41 native codex model attribution | expected={EXPECTED_MODEL:?} actual={actual:?}"
    );
    println!(
        "plain: {label} | expected_model={EXPECTED_MODEL} actual_models={actual:?} replay_child_exit={replay_child_exit} authored_schedule_ms={authored_schedule_ms:?} measured_pipe_elapsed_ms={measured_pipe_elapsed_ms}"
    );

    if let Ok(evidence_path) = std::env::var("BOOP_F41_EVIDENCE") {
        let evidence = serde_json::json!({
            "probe": "f41-native-model-attribution",
            "fixture": "crates/boop-harness/tests/fixtures/transcripts/codex/codex-native-model.jsonl",
            "fixture_bytes": FIXTURE.len(),
            "fixture_lines": FIXTURE.lines().count(),
            "replay": {
                "channels": "real OS pipe",
                "schedule_events": chunks.len() + 1,
                "authored_schedule_ms": authored_schedule_ms,
                "schedule_semantics": "authored logical schedule; drive() does not wall-clock pace it",
                "measured_pipe_elapsed_ms": measured_pipe_elapsed_ms,
                "recorded_equals_replayed": recorded_equals_replayed,
                "replay_child_exit": replay_child_exit,
            },
            "command": "BOOP_F41_EVIDENCE=<path> BOOP_SOURCE_SHA=<sha> cargo test -p boop-harness --test native_model_receipt -j2 -- --nocapture",
            "source_sha": std::env::var("BOOP_SOURCE_SHA").unwrap_or_else(|_| "unknown".into()),
            "expected_model": EXPECTED_MODEL,
            "actual_models": actual,
            "result": if passed { "GREEN_FIXED" } else { "RED_REPRO" },
            "fidelity": "pipe/adapter replay; no PTY or screen emulator exercised",
        });
        std::fs::write(
            &evidence_path,
            serde_json::to_string_pretty(&evidence).unwrap() + "\n",
        )
        .expect("write evidence");
    }

    let _ = std::fs::remove_file(&transcript);
    let _ = std::fs::remove_file(&db_path);

    assert_eq!(
        actual,
        vec![(EXPECTED_MODEL.to_owned(), 2)],
        "turn_context.payload.model is the per-turn attribution"
    );
}
