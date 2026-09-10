//! Relative-time replay through real child stdin/stdout pipes, driven by the
//! reusable `boop_harness::harness::replay` seam.
//!
//! Boundaries named by these tests:
//! - recorded cast bytes -> real OS pipe -> child stdin/stdout -> bytes, order,
//!   EOF, exit code
//! - scrubbed transcript bytes -> real OS pipe -> file -> real Codex adapter
//!   `read_from` -> `AgentEvent` sequence, compared against a direct read

use std::path::PathBuf;
use std::process::{Command, Stdio};

use boop_harness::harness::replay::{ReplayChannel, drive, parse_cast};
use boop_harness::{Harness, HarnessId, SessionRef};

fn spawn_shell(script: &str) -> std::process::Child {
    Command::new("sh")
        .args(["-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn sh child")
}

#[test]
fn synthetic_cast_replays_relative_schedule_through_os_pipes() {
    let cast = include_str!("fixtures/terminal_pipe_replay.cast");
    let schedule = parse_cast(cast).expect("parse cast");
    let repeat = parse_cast(cast).expect("parse cast twice");
    assert_eq!(schedule, repeat, "parse is deterministic");

    assert_eq!(
        schedule
            .iter()
            .map(|event| (event.at_ms, event.channel.clone(), event.source_index))
            .collect::<Vec<_>>(),
        vec![
            (0, ReplayChannel::Marker, 2),
            (0, ReplayChannel::Resize { cols: 80, rows: 24 }, 4),
            (100, ReplayChannel::Input, 0),
            (100, ReplayChannel::Output, 1),
            (100, ReplayChannel::Output, 3),
            (200, ReplayChannel::Marker, 5),
        ]
    );

    let mut child = spawn_shell(
        "IFS= read -r line; printf 'screen-ready\\n'; printf 'echo:%s\\n' \"$line\"; exit 7",
    );
    let mut child_stdin = Some(child.stdin.take().expect("child stdin pipe"));
    let mut child_stdout = child.stdout.take().expect("child stdout pipe");

    let outcome = drive(&schedule, &mut child_stdin, &mut child_stdout, "replay-eof")
        .expect("drive schedule through pipes");
    let exit_code = child.wait().expect("child exits").code();

    assert_eq!(outcome.input, b"ping\n");
    assert_eq!(outcome.recorded_output, b"screen-ready\necho:ping\n");
    assert_eq!(outcome.actual_output, outcome.recorded_output);
    assert_eq!(
        outcome.markers,
        vec![(0, "replay-start".into()), (200, "replay-eof".into())]
    );
    assert_eq!(outcome.resizes, vec![(0, 80, 24)]);
    assert!(outcome.input_closed);
    assert_eq!(exit_code, Some(7));
}

/// The reusable seam feeding a real adapter: a scrubbed transcript's bytes are
/// cut into timed chunks, replayed through an OS pipe into a file, then decoded
/// by the real `Codex` adapter. The decoded events must equal a direct read.
#[test]
fn codex_transcript_bytes_replayed_through_os_pipe_decode_identically() {
    let fixture = include_str!("fixtures/codex_replay.jsonl");
    let fixture_bytes = fixture.as_bytes();
    let lines: Vec<&[u8]> = fixture_bytes
        .split_inclusive(|&byte| byte == b'\n')
        .collect();
    let first = lines.first().expect("first fixture line");
    let second = lines.get(1).expect("second fixture line");
    let split = first.len() / 2;
    let chunks: Vec<&[u8]> = vec![&first[..split], &first[split..], second];

    let header = serde_json::json!({"version": 2, "width": 80, "height": 24});
    let mut cast = serde_json::to_string(&header).unwrap();
    for (index, chunk) in chunks.iter().enumerate() {
        let text = std::str::from_utf8(chunk).expect("fixture is utf-8");
        let event = serde_json::json!([index as f64 * 0.05, "i", text]);
        cast.push('\n');
        cast.push_str(&serde_json::to_string(&event).unwrap());
    }
    cast.push('\n');
    cast.push_str(&serde_json::to_string(&serde_json::json!([0.5, "m", "replay-eof"])).unwrap());

    let schedule = parse_cast(&cast).expect("parse transcript schedule");
    assert_eq!(
        schedule
            .iter()
            .filter(|event| matches!(event.channel, ReplayChannel::Input))
            .map(|event| event.data.as_slice())
            .collect::<Vec<_>>(),
        chunks,
        "chunk splitting is preserved in schedule order"
    );

    let transcript_path =
        std::env::temp_dir().join(format!("boop_codex_replay_{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&transcript_path);
    let mut child = Command::new("sh")
        .args(["-c", "cat > \"$1\"", "sh"])
        .arg(&transcript_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn cat child");
    let mut child_stdin = Some(child.stdin.take().expect("child stdin pipe"));
    let mut child_stdout = child.stdout.take().expect("child stdout pipe");
    let outcome = drive(&schedule, &mut child_stdin, &mut child_stdout, "replay-eof")
        .expect("drive transcript through pipe");
    let exit_code = child.wait().expect("cat exits").code();
    assert_eq!(exit_code, Some(0));
    assert!(outcome.input_closed);

    let written = std::fs::read(&transcript_path).expect("read replayed transcript");
    assert_eq!(written, fixture_bytes, "recorded bytes survive the pipe");

    let direct = codex_session(&fixture_path(), fixture_bytes.len());
    let replayed = codex_session(&transcript_path, written.len());
    let codex = boop_harness::harness::codex::Codex;

    let direct_chunk = codex.read_from(&direct, 0).expect("direct decode");
    let replayed_chunk = codex.read_from(&replayed, 0).expect("replayed decode");

    assert_eq!(direct_chunk.skipped, 0);
    assert_eq!(replayed_chunk.skipped, 0);
    assert_eq!(
        serde_json::to_value(&direct_chunk.events).unwrap(),
        serde_json::to_value(&replayed_chunk.events).unwrap(),
        "real adapter decodes replayed bytes identically"
    );
    assert_eq!(replayed_chunk.next_offset, fixture_bytes.len() as u64);

    let resumed = codex
        .read_from(&replayed, replayed_chunk.next_offset)
        .expect("resume decode");
    assert!(resumed.events.is_empty());

    let _ = std::fs::remove_file(&transcript_path);
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex_replay.jsonl")
}

fn codex_session(path: &PathBuf, size: usize) -> SessionRef {
    SessionRef {
        harness: HarnessId::Codex,
        session_id: "S-0001".into(),
        nickname: "S-0001".into(),
        path: path.clone(),
        cwd: Some("/Users/dev/replay".into()),
        git_branch: None,
        modified_ms: 0,
        size: size as u64,
        tmux: None,
        tmux_socket: None,
        parent: Some("S-0000".into()),
    }
}
