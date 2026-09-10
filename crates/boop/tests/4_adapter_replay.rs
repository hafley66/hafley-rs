//! Offline replay proof for every real `Harness::read_from` adapter.
//!
//! `2_replay_fixture.rs` covers Codex and `3_terminal_pipe_replay.rs` drives the
//! reusable `replay` seam over real OS pipes. This file extends the same seam to
//! the remaining three adapters: Claude and Kimi tail scrubbed JSONL by byte
//! offset, OpenCode tails `message.rowid` in a real SQLite store. Each test
//! commits only fixture bytes, touches no model and no network, and asserts the
//! same properties: decoded `AgentEvent` equality, repeated-run equality,
//! relative ordering and timing, and cursor resume.

use std::path::PathBuf;

use boop_harness::harness::opencode::Opencode;
use boop_harness::{Harness, HarnessId, SessionRef};
use serde_json::json;

/// Byte offset of each line's first byte, mirroring `tail::CompleteLine.start`.
fn line_offsets(document: &str) -> Vec<u64> {
    let mut offsets = Vec::new();
    let mut offset = 0u64;
    for line in document.split_inclusive('\n') {
        offsets.push(offset);
        offset += line.len() as u64;
    }
    offsets
}

fn read(chunk: &boop_harness::harness::ReadChunk) -> serde_json::Value {
    serde_json::to_value(&chunk.events).expect("events serialize")
}

/// `(raw_line_offset, ts_ms)` in schedule order: the two ordering facts every
/// adapter carries.
fn ordering(chunk: &boop_harness::harness::ReadChunk) -> Vec<(u64, u64)> {
    chunk
        .events
        .iter()
        .map(|event| (event.raw_line_offset, event.ts_ms))
        .collect()
}

/// Both facts must rise with the schedule; the tie-break is not exercised by a
/// well-formed transcript, so strictness is the correct claim.
fn assert_ordered(pairs: &[(u64, u64)], context: &str) {
    for pair in pairs.windows(2) {
        assert!(
            pair[0].0 < pair[1].0 && pair[0].1 < pair[1].1,
            "{context} ordering: {pairs:?}"
        );
    }
}

fn file_session(harness: HarnessId, session_id: &str, path: PathBuf, size: u64) -> SessionRef {
    SessionRef {
        harness,
        session_id: session_id.into(),
        nickname: session_id.into(),
        path,
        cwd: Some("/Users/dev/replay".into()),
        git_branch: Some("dev".into()),
        modified_ms: 0,
        size,
        tmux: None,
        tmux_socket: None,
        parent: None,
    }
}

#[test]
fn claude_fixture_replays_exact_events_orders_timing_and_resumes_at_the_cursor() {
    let fixture = include_str!("fixtures/claude_replay.jsonl");
    let offsets = line_offsets(fixture);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude_replay.jsonl");
    let session = file_session(HarnessId::Claude, "S-0001", path, fixture.len() as u64);
    let adapter = boop_harness::harness::claude::Claude;

    let first = adapter.read_from(&session, 0).expect("read fixture");
    let repeat = adapter.read_from(&session, 0).expect("repeat fixture read");

    assert_eq!(first.skipped, 0);
    assert!(!first.reset);
    assert_eq!(first.next_offset, fixture.len() as u64);
    assert_eq!(first.events.len(), 3);
    assert_eq!(
        read(&first),
        json!([
            {
                "harness": "claude",
                "session_id": "S-0001",
                "ts_ms": 1767225600000u64,
                "uuid": "S-0001-u1",
                "parent_uuid": null,
                "cwd": "/Users/dev/replay",
                "git_branch": "dev",
                "record_type": "assistant",
                "tool_name": "Read",
                "paths": [{"path": "/Users/dev/replay/src/lib.rs", "access": "Read"}],
                "urls": [],
                "raw_line_offset": offsets[0]
            },
            {
                "harness": "claude",
                "session_id": "S-0001",
                "ts_ms": 1767225601250u64,
                "uuid": "S-0001-u2",
                "parent_uuid": "S-0001-u1",
                "cwd": "/Users/dev/replay",
                "git_branch": "dev",
                "record_type": "user",
                "tool_name": null,
                "paths": [],
                "urls": [],
                "raw_line_offset": offsets[1]
            },
            {
                "harness": "claude",
                "session_id": "S-0001",
                "ts_ms": 1767225602500u64,
                "uuid": "S-0001-u3",
                "parent_uuid": "S-0001-u2",
                "cwd": "/Users/dev/replay",
                "git_branch": "dev",
                "record_type": "assistant",
                "tool_name": null,
                "paths": [],
                "urls": [],
                "raw_line_offset": offsets[2]
            }
        ])
    );
    assert_eq!(
        read(&first),
        read(&repeat),
        "repeated runs decode identically"
    );

    let pairs = ordering(&first);
    assert_ordered(&pairs, "claude");
    let deltas = pairs
        .windows(2)
        .map(|pair| pair[1].1 - pair[0].1)
        .collect::<Vec<_>>();
    assert_eq!(deltas, vec![1250, 1250], "relative timing metadata");

    let first_line_end = offsets[1];
    let resumed = adapter
        .read_from(&session, first_line_end)
        .expect("resume mid-file");
    assert_eq!(resumed.events.len(), 2);
    assert_eq!(read(&resumed), json!(first.events[1..].to_vec()));
    assert_eq!(resumed.next_offset, fixture.len() as u64);

    let exhausted = adapter
        .read_from(&session, first.next_offset)
        .expect("resume at cursor");
    assert!(exhausted.events.is_empty());
    assert_eq!(exhausted.next_offset, first.next_offset);
    assert_eq!(exhausted.skipped, 0);
    assert!(!exhausted.reset);
}

#[test]
fn kimi_fixture_replays_exact_events_orders_timing_and_resumes_at_the_cursor() {
    let fixture = include_str!("fixtures/kimi_replay.jsonl");
    let offsets = line_offsets(fixture);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kimi_replay.jsonl");
    let session = file_session(HarnessId::Kimi, "S-0001", path, fixture.len() as u64);
    let adapter = boop_harness::harness::kimi::Kimi;

    let first = adapter.read_from(&session, 0).expect("read fixture");
    let repeat = adapter.read_from(&session, 0).expect("repeat fixture read");

    assert_eq!(first.skipped, 0);
    assert!(!first.reset);
    assert_eq!(first.next_offset, fixture.len() as u64);
    assert_eq!(first.events.len(), 3);
    assert_eq!(
        read(&first),
        json!([
            {
                "harness": "kimi",
                "session_id": "S-0001",
                "ts_ms": 1767225600000u64,
                "uuid": null,
                "parent_uuid": null,
                "cwd": "/Users/dev/replay",
                "git_branch": "dev",
                "record_type": "context.append_message",
                "tool_name": null,
                "paths": [],
                "urls": [],
                "raw_line_offset": offsets[0]
            },
            {
                "harness": "kimi",
                "session_id": "S-0001",
                "ts_ms": 1767225601250u64,
                "uuid": null,
                "parent_uuid": null,
                "cwd": "/Users/dev/replay",
                "git_branch": "dev",
                "record_type": "tool.call",
                "tool_name": "Read",
                "paths": [{"path": "src/lib.rs", "access": "Read"}],
                "urls": [],
                "raw_line_offset": offsets[1]
            },
            {
                "harness": "kimi",
                "session_id": "S-0001",
                "ts_ms": 1767225602500u64,
                "uuid": null,
                "parent_uuid": null,
                "cwd": "/Users/dev/replay",
                "git_branch": "dev",
                "record_type": "tool.result",
                "tool_name": null,
                "paths": [],
                "urls": [],
                "raw_line_offset": offsets[2]
            }
        ])
    );
    assert_eq!(
        read(&first),
        read(&repeat),
        "repeated runs decode identically"
    );

    let pairs = ordering(&first);
    assert_ordered(&pairs, "kimi");
    let deltas = pairs
        .windows(2)
        .map(|pair| pair[1].1 - pair[0].1)
        .collect::<Vec<_>>();
    assert_eq!(deltas, vec![1250, 1250], "relative timing metadata");

    let first_line_end = offsets[1];
    let resumed = adapter
        .read_from(&session, first_line_end)
        .expect("resume mid-file");
    assert_eq!(resumed.events.len(), 2);
    assert_eq!(read(&resumed), json!(first.events[1..].to_vec()));
    assert_eq!(resumed.next_offset, fixture.len() as u64);

    let exhausted = adapter
        .read_from(&session, first.next_offset)
        .expect("resume at cursor");
    assert!(exhausted.events.is_empty());
    assert_eq!(exhausted.next_offset, first.next_offset);
    assert_eq!(exhausted.skipped, 0);
    assert!(!exhausted.reset);
}

/// The db path for one test process, built from the committed SQL fixture. No
/// binary store is committed; the shape is opencode's own `session`/`message`/
/// `part` tables, and `read_from` opens it read-only and tails rowid.
fn opencode_replay_db(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "boop_opencode_replay_{}_{}.db",
        tag,
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let connection = rusqlite::Connection::open(&path).expect("create replay db");
    connection
        .execute_batch(include_str!("fixtures/opencode_replay.sql"))
        .expect("apply replay sql");
    drop(connection);
    path
}

#[test]
fn opencode_fixture_replays_exact_events_orders_timing_and_resumes_at_the_rowid_cursor() {
    let db = opencode_replay_db("read");
    let session = file_session(HarnessId::Opencode, "ses_replay_0001", db.clone(), 0);
    let adapter = Opencode;

    let first = adapter.read_from(&session, 0).expect("read fixture");
    let repeat = adapter.read_from(&session, 0).expect("repeat fixture read");

    assert_eq!(first.skipped, 0);
    assert!(!first.reset);
    assert_eq!(first.events.len(), 3);
    assert_eq!(first.next_offset, 3, "cursor is the last message rowid");
    assert_eq!(
        read(&first),
        json!([
            {
                "harness": "opencode",
                "session_id": "ses_replay_0001",
                "ts_ms": 1767225600000u64,
                "uuid": "msg_replay_0001",
                "parent_uuid": null,
                "cwd": "/Users/dev/replay",
                "git_branch": null,
                "record_type": "user",
                "tool_name": null,
                "paths": [],
                "urls": [],
                "raw_line_offset": 1
            },
            {
                "harness": "opencode",
                "session_id": "ses_replay_0001",
                "ts_ms": 1767225601250u64,
                "uuid": "msg_replay_0002",
                "parent_uuid": null,
                "cwd": "/Users/dev/replay",
                "git_branch": null,
                "record_type": "assistant",
                "tool_name": null,
                "paths": [],
                "urls": [],
                "raw_line_offset": 2
            },
            {
                "harness": "opencode",
                "session_id": "ses_replay_0001",
                "ts_ms": 1767225602500u64,
                "uuid": "msg_replay_0003",
                "parent_uuid": null,
                "cwd": "/Users/dev/replay",
                "git_branch": null,
                "record_type": "assistant",
                "tool_name": null,
                "paths": [],
                "urls": [],
                "raw_line_offset": 3
            }
        ])
    );
    assert_eq!(
        read(&first),
        read(&repeat),
        "repeated runs decode identically"
    );

    let pairs = ordering(&first);
    assert_ordered(&pairs, "opencode");
    let deltas = pairs
        .windows(2)
        .map(|pair| pair[1].1 - pair[0].1)
        .collect::<Vec<_>>();
    assert_eq!(deltas, vec![1250, 1250], "relative timing metadata");

    let resumed = adapter.read_from(&session, 1).expect("resume mid-store");
    assert_eq!(resumed.events.len(), 2);
    assert_eq!(read(&resumed), json!(first.events[1..].to_vec()));
    assert_eq!(resumed.next_offset, 3);

    let exhausted = adapter
        .read_from(&session, first.next_offset)
        .expect("resume at cursor");
    assert!(exhausted.events.is_empty());
    assert_eq!(exhausted.next_offset, first.next_offset);
    assert_eq!(exhausted.skipped, 0);
    assert!(!exhausted.reset);

    let _ = std::fs::remove_file(db);
}

/// A half-written trailing line is the byte-plane chunk boundary: it must stay
/// unconsumed and uncounted until its newline arrives, then decode exactly once.
/// Claude is the witness; Kimi shares `tail::read_complete_lines`.
#[test]
fn claude_partial_trailing_line_is_held_until_terminated_then_decoded_once() {
    use std::io::Write;

    let fixture = include_str!("fixtures/claude_replay.jsonl");
    let path =
        std::env::temp_dir().join(format!("boop_claude_partial_{}.jsonl", std::process::id()));
    std::fs::write(&path, fixture).expect("seed transcript");
    let session = file_session(
        HarnessId::Claude,
        "S-0001",
        path.clone(),
        fixture.len() as u64,
    );
    let adapter = boop_harness::harness::claude::Claude;

    let prefix = adapter
        .read_from(&session, 0)
        .expect("read complete prefix");
    assert_eq!(prefix.next_offset, fixture.len() as u64);
    let boundary = fixture.len() as u64;

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open for append");
    file.write_all(
        b"{\"type\":\"assistant\",\"uuid\":\"S-0001-u4\",\"sessionId\":\"S-0001\",\"timestamp\":\"2026-01-01T00:00:03.750Z\",\"message\":{\"role\":\"assistant\",\"content\":[]}",
    )
    .expect("append partial line");
    drop(file);

    let held = adapter
        .read_from(&session, boundary)
        .expect("read partial tail");
    assert!(held.events.is_empty(), "partial line decodes nothing");
    assert_eq!(held.next_offset, boundary, "partial line is not consumed");
    assert_eq!(held.skipped, 0, "partial line is not counted");
    assert!(!held.reset);

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open for append");
    file.write_all(b"}\n").expect("terminate partial line");
    drop(file);

    let completed = adapter.read_from(&session, boundary).expect("read tail");
    assert_eq!(completed.events.len(), 1);
    assert_eq!(completed.events[0].uuid.as_deref(), Some("S-0001-u4"));
    assert_eq!(completed.events[0].ts_ms, 1767225603750);
    assert_eq!(
        completed.next_offset,
        std::fs::metadata(&path).unwrap().len()
    );

    let _ = std::fs::remove_file(path);
}
