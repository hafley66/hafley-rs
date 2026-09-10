//! Offline replay proof for the real Codex transcript adapter.

use boop_harness::harness::SessionTopology;
use boop_harness::{Harness, HarnessId, SessionRef};
use serde_json::json;
use std::path::PathBuf;

#[test]
fn codex_fixture_replays_exact_events_and_resumes_at_the_byte_cursor() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex_replay.jsonl");
    let fixture = include_str!("fixtures/codex_replay.jsonl");
    let session = SessionRef {
        harness: HarnessId::Codex,
        session_id: "S-0001".into(),
        nickname: "S-0001".into(),
        path,
        cwd: Some("/Users/dev/replay".into()),
        git_branch: None,
        modified_ms: 0,
        size: fixture.len() as u64,
        tmux: None,
        tmux_socket: None,
        parent: Some("S-0000".into()),
    };
    let codex = boop_harness::harness::codex::Codex;

    let first = codex.read_from(&session, 0).expect("read fixture");
    let repeat = codex.read_from(&session, 0).expect("repeat fixture read");

    assert_eq!(first.skipped, 0);
    assert!(!first.reset);
    assert_eq!(first.next_offset, fixture.len() as u64);
    assert_eq!(first.events.len(), 2);
    assert_eq!(
        serde_json::to_value(&first.events).unwrap(),
        json!([
            {
                "harness": "codex",
                "session_id": "S-0001",
                "ts_ms": 1767225600000u64,
                "uuid": "S-0001",
                "parent_uuid": null,
                "cwd": "/Users/dev/replay",
                "git_branch": null,
                "record_type": "session_meta",
                "tool_name": null,
                "paths": [],
                "urls": [],
                "raw_line_offset": 0
            },
            {
                "harness": "codex",
                "session_id": "S-0001",
                "ts_ms": 1767225604630u64,
                "uuid": null,
                "parent_uuid": null,
                "cwd": "/Users/dev/replay",
                "git_branch": null,
                "record_type": "task_complete",
                "tool_name": null,
                "paths": [],
                "urls": [],
                "raw_line_offset": 380
            }
        ])
    );
    assert_eq!(
        serde_json::to_value(&first.events).unwrap(),
        serde_json::to_value(&repeat.events).unwrap()
    );

    assert_eq!(session.parent.as_deref(), Some("S-0000"));
    assert_eq!(
        codex.session_topology(&session),
        SessionTopology::NativeChild {
            parent_session: "S-0000".into()
        }
    );

    let resumed = codex
        .read_from(&session, first.next_offset)
        .expect("resume fixture read");
    assert!(resumed.events.is_empty());
    assert_eq!(resumed.next_offset, first.next_offset);
    assert_eq!(resumed.skipped, 0);
    assert!(!resumed.reset);
}
