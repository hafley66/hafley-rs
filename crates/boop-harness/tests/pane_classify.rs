//! pane::classify on stored turns, and the frame those turns project onto a claude screen.

use boop_harness::pane::{classify, project, Layout, Mode, Options};
use boop_mux::{
    rows_from_capture, History, Screen, TerminalSize, TerminalSnapshot, TerminalTarget,
};
use boop_store::ident::{project_transcript, sync_session_with, Store, TurnQuery};
use boop_store::rows::TurnRow;
use boop_turnstrip::ToolGap;

#[test]
fn boop_envelopes_change_presentation_only() {
    let scratch = tempfile::tempdir().unwrap();
    let store = Store::open(scratch.path().join("boop.db")).unwrap();
    let texts = [
        "[boop m1 from coordinator]\nactual user content\nsecond line",
        "[boop m2 from feature/tls]",
        "[ordinary brackets] actual content",
        "<system-reminder>injected text</system-reminder>",
        "<user-data>keep this</user-data>",
    ];
    let raw: Vec<TurnRow> = texts
        .iter()
        .enumerate()
        .map(|(index, text)| TurnRow {
            session: "fixture".into(),
            harness: "claude".into(),
            turn: index as i64 + 1,
            ts: 0,
            role: "user".into(),
            said: (*text).into(),
        })
        .collect();
    let mut shown = raw.clone();
    classify(&store, &mut shown).unwrap();
    assert_eq!(
        shown
            .iter()
            .map(|row| (row.role.as_str(), row.said.as_str()))
            .collect::<Vec<_>>(),
        [
            ("user", "actual user content\nsecond line"),
            ("meta", ""),
            ("user", texts[2]),
            ("user", texts[3]),
            ("user", texts[4]),
        ]
    );
    assert_eq!(
        raw.iter().map(|row| row.said.as_str()).collect::<Vec<_>>(),
        texts
    );
}

#[test]
fn claude_mixed_blocks_preserve_text_tools_and_usage_provenance() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("claude.jsonl");
    std::fs::write(&path, include_str!("fixtures/1_claude-mixed-records.jsonl")).unwrap();
    let store = Store::open(scratch.path().join("boop.db")).unwrap();
    let session = boop_harness::SessionRef {
        harness: boop_harness::HarnessId::Claude,
        session_id: "fixture".into(),
        nickname: "fixture".into(),
        path,
        cwd: None,
        git_branch: None,
        modified_ms: 0,
        size: 0,
        tmux: None,
        tmux_socket: None,
        parent: None,
    };
    sync_session_with(&store, &session, None, 0, |store, session, cursor| {
        project_transcript(store, session, cursor.offset)
    })
    .unwrap();
    let query = TurnQuery {
        session: Some("fixture".into()),
        ..Default::default()
    };
    let mut rows = store.turn_rows(&query).unwrap();
    classify(&store, &mut rows).unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| (row.turn, row.role.as_str(), row.said.as_str()))
            .collect::<Vec<_>>(),
        [
            (1, "user", "okay now try"),
            (2, "thinking", ""),
            (3, "tool", "mcp__bewpp__browser_status"),
            (4, "tool", "mcp__bewpp__tabs_list"),
            (5, "thinking", ""),
            (6, "tool", "mcp__bewpp__page_navigate"),
            (7, "thinking", ""),
            (8, "tool", "Bash"),
            (
                9,
                "assistant",
                "Blocked at the extension. Navigation requires site permission."
            ),
            (10, "assistant", "Reading the extension configuration."),
            (11, "tool", "Read"),
            (12, "assistant", ""),
        ]
    );
    assert_eq!(
        store.turn_rows(&query).unwrap()[1].role,
        "assistant",
        "presentation leaves the ledger unchanged"
    );

    let screen = [
        "❯ okay now try",
        "",
        "Read 1 file, called bewpp 3 times, ran 1 shell command",
        "",
        "⏺ Blocked at the extension. Navigation requires site permission.",
        "",
        "⏺ Reading the extension configuration.",
    ];
    let text: String = screen.iter().map(|row| format!("{row}\n")).collect();
    let size = TerminalSize {
        columns: 100,
        rows: screen.len() as u16,
    };
    let snapshot = TerminalSnapshot {
        target: TerminalTarget {
            host: "tmux".into(),
            terminal: "%1".into(),
            incarnation: 1,
        },
        generation: 0,
        size,
        screen: Screen::Primary,
        history: History::Retained {
            rows: 0,
            capacity: 2000,
        },
        cursor: None,
        scroll: 0,
        rows: rows_from_capture(&text, &text, size),
    };
    let frame = project(
        &snapshot,
        "fixture",
        rows,
        &Default::default(),
        &Options {
            mode: Mode::Recent,
            ..Default::default()
        },
    );
    let layout = frame.layout.unwrap();
    assert_eq!(
        layout
            .squares()
            .iter()
            .map(|square| (square.id.as_str(), square.active))
            .collect::<Vec<_>>(),
        [
            ("fixture:1", true),
            ("fixture:9", true),
            ("fixture:10", true),
            ("fixture:12", false)
        ]
    );
    let tool = frame.turns.iter().find(|turn| turn.role == "tool").unwrap();
    assert_eq!((tool.turn, tool.anchor_start, tool.anchor_end), (8, 2, 2));
    let Layout::Recent(recent) = layout else {
        panic!("recent mode");
    };
    assert_eq!(
        recent.gap,
        Some(ToolGap {
            before_id: Some("fixture:1".into()),
            after_id: Some("fixture:9".into()),
            start_row: 2,
            end_row: 2
        })
    );
}
