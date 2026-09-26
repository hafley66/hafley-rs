//! A claude tool call is drawn as `Name(arg)`; the frame places it on that row.

use boop_harness::pane::{frame_turns, project, Options};
use boop_mux::{rows_from_capture, History, Screen, TerminalSize, TerminalSnapshot, TerminalTarget};
use boop_store::ident::{project_transcript, sync_session_with, Store};

const TRANSCRIPT: &str = concat!(
    r#"{"type":"user","timestamp":"2026-09-26T10:00:00Z","message":{"content":[{"type":"text","text":"list the repo"}]}}"#, "\n",
    r#"{"type":"assistant","timestamp":"2026-09-26T10:00:01Z","message":{"id":"r1","content":[{"type":"tool_use","id":"b1","name":"Bash","input":{"command":"cd ~/projects/hafley-rs && ls -la crates\necho done"}}]}}"#, "\n",
    r#"{"type":"assistant","timestamp":"2026-09-26T10:00:02Z","message":{"id":"r2","content":[{"type":"tool_use","id":"b2","name":"Read","input":{"file_path":"/Users/someone/projects/hafley-rs/crates/boop-harness/src/pane.rs"}}]}}"#, "\n",
    r#"{"type":"assistant","timestamp":"2026-09-26T10:00:03Z","message":{"id":"r3","content":[{"type":"text","text":"The crates are listed above."}]}}"#, "\n",
);

fn snapshot(screen: &[&str]) -> TerminalSnapshot {
    let text: String = screen.iter().map(|row| format!("{row}\n")).collect();
    let size = TerminalSize { columns: 120, rows: screen.len() as u16 };
    TerminalSnapshot {
        target: TerminalTarget { host: "tmux".into(), terminal: "%1".into(), incarnation: 1 },
        generation: 0,
        size,
        screen: Screen::Primary,
        history: History::Retained { rows: 0, capacity: 2000 },
        cursor: None,
        scroll: 0,
        rows: rows_from_capture(&text, &text, size),
    }
}

#[test]
fn tool_calls_land_on_their_header_rows() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("claude.jsonl");
    std::fs::write(&path, TRANSCRIPT).unwrap();
    let store = Store::open(scratch.path().join("boop.db")).unwrap();
    let session = boop_harness::SessionRef {
        harness: boop_harness::HarnessId::Claude,
        session_id: "tools".into(),
        nickname: "tools".into(),
        path,
        cwd: None,
        git_branch: None,
        modified_ms: 0,
        size: 0,
        tmux: None,
        tmux_socket: None,
        parent: None,
    };
    sync_session_with(&store, &session, None, 0, |store, session, cursor| project_transcript(store, session, cursor.offset)).unwrap();

    let screen = [
        "❯ list the repo",
        "",
        "⏺ Bash(cd ~/projects/hafley-rs && ls -la crates…)",
        "  ⎿  total 8",
        "",
        "⏺ Read(~/projects/hafley-rs/crates/boop-harness/src/pane.rs)",
        "  ⎿  Read 412 lines",
        "",
        "⏺ The crates are listed above.",
    ];
    let frame = project(&snapshot(&screen), "tools", frame_turns(&store, "tools").unwrap(), &Options::default());
    let placed: Vec<(i64, &str, usize)> =
        frame.turns.iter().map(|turn| (turn.turn, turn.role.as_str(), turn.anchor_start)).collect();
    assert_eq!(placed, [(1, "user", 0), (2, "tool", 2), (3, "tool", 5), (4, "assistant", 8)], "{:#?}", frame.turns);
}
