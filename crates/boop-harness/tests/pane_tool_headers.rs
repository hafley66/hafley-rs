//! A claude tool call is drawn as `Name(arg)`; the frame places it on that row.

use boop_harness::harness::claude_summary;
use boop_harness::pane::{frame_turns, project, tool_args, Options};
use boop_mux::{
    rows_from_capture, History, Screen, TerminalSize, TerminalSnapshot, TerminalTarget,
};
use boop_store::ident::{project_transcript, sync_session_with, Store};
use boop_turnvis::{locate_visible_turns_with, BoopTurn, LogicalLine};

const TRANSCRIPT: &str = concat!(
    r#"{"type":"user","timestamp":"2026-09-26T10:00:00Z","message":{"content":[{"type":"text","text":"list the repo"}]}}"#,
    "\n",
    r#"{"type":"assistant","timestamp":"2026-09-26T10:00:01Z","message":{"id":"r1","content":[{"type":"tool_use","id":"b1","name":"Bash","input":{"command":"cd ~/projects/hafley-rs && ls -la crates\necho done"}}]}}"#,
    "\n",
    r#"{"type":"assistant","timestamp":"2026-09-26T10:00:02Z","message":{"id":"r2","content":[{"type":"tool_use","id":"b2","name":"Read","input":{"file_path":"/Users/someone/projects/hafley-rs/crates/boop-harness/src/pane.rs"}}]}}"#,
    "\n",
    r#"{"type":"assistant","timestamp":"2026-09-26T10:00:03Z","message":{"id":"r3","content":[{"type":"text","text":"The crates are listed above."}]}}"#,
    "\n",
);

fn snapshot(screen: &[&str]) -> TerminalSnapshot {
    let text: String = screen.iter().map(|row| format!("{row}\n")).collect();
    let size = TerminalSize {
        columns: 120,
        rows: screen.len() as u16,
    };
    TerminalSnapshot {
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
    sync_session_with(&store, &session, None, 0, |store, session, cursor| {
        project_transcript(store, session, cursor.offset)
    })
    .unwrap();

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
    let rows = frame_turns(&store, "tools").unwrap();
    let args = tool_args(&store, "tools", &rows).unwrap();
    let frame = project(
        &snapshot(&screen),
        "tools",
        rows,
        &args,
        &Options::default(),
    );
    let placed: Vec<(i64, &str, usize)> = frame
        .turns
        .iter()
        .map(|turn| (turn.turn, turn.role.as_str(), turn.anchor_start))
        .collect();
    assert_eq!(
        placed,
        [
            (1, "user", 0),
            (2, "tool", 2),
            (3, "tool", 5),
            (4, "assistant", 8)
        ],
        "{:#?}",
        frame.turns
    );
}

// Rows copied from a live claude pane (2026-09-26): a hard-wrapped reply and two expanded tool
// blocks stored the harness reader's way, `[Name] {json}` (capped, so the JSON need not parse).
#[test]
fn claude_json_tool_lines_anchor_their_blocks() {
    let line = |row: usize, text: &str| LogicalLine {
        text: text.to_owned(),
        start: row,
        end: row,
    };
    let turn = |turn: i64, said: &str| BoopTurn {
        session: "s".to_owned(),
        harness: "claude".to_owned(),
        turn,
        ts: turn,
        role: "assistant".to_owned(),
        said: said.to_owned(),
        aliases: claude_summary::screen_lines("assistant", said, None),
    };
    let screen = [
        line(0, "⏺ The test-budget lane has retired. Its worktree is clean, and one message to it would bring back the same"),
        line(1, "  conversation. Nothing is running now; the two items above are still"),
        line(2, "  waiting on you."),
        line(3, ""),
        line(4, "⏺ Bash(cd ~/projects/instant && git log --oneline -4 && git revert --no-edit -m 1 2e16a23c 2>&1 | tail -2 && git lo…)"),
        line(5, "  ⎿  9164a7e7 plans: one turn projection brief"),
        line(6, ""),
        line(7, "⏺ Write(~/projects/hafley-rxjs/plans/2026-09-26-tsp-ui-names.PLAN.md)"),
        line(8, "  ⎿  Wrote 285 lines to ../hafley-rxjs/plans/2026-09-26-tsp-ui-names.PLAN.md"),
    ];
    let turns = [
        turn(11, "**Done**\n- Waves 1 and 2 are merged in instant and hafley-rxjs.\n\n**Not done**\n- 6 decisions waiting on you."),
        turn(518, "The test-budget lane has retired. Its worktree is clean, and one message to it would bring back the same conversation. Nothing is running now; the two items above are still waiting on you."),
        turn(520, "[Bash] {\"command\":\"cd ~/projects/instant && git log --oneline -4 && git revert --no-edit -m 1 2e16a23c 2>&1 | tail -2 && git log --oneline -1\",\"description\":\"Revert cutover merge on instant main\"}"),
        turn(522, "[Write] {\"content\":\"# Plan: TypeSpec-first UI names\\n\\nStatus: draft.\",\"file_path\":\"/Users/chrishafley/projects/hafley-rxjs/plans/2026-09-26-tsp-ui-names.PLAN.md\""),
    ];
    let found: Vec<String> =
        locate_visible_turns_with(&screen, &turns, Some(claude_summary::anchor))
            .iter()
            .map(|t| {
                format!(
                    "{} anchor {}..{} buffer {}..{} {:?}",
                    t.turn,
                    t.anchor_start,
                    t.anchor_end,
                    t.buffer_start,
                    t.buffer_end,
                    t.confidence
                )
            })
            .collect();
    assert_eq!(
        found.join("\n"),
        [
            "518 anchor 0..2 buffer 0..2 Anchored",
            "520 anchor 4..4 buffer 4..5 Extended",
            "522 anchor 7..7 buffer 7..8 Extended"
        ]
        .join("\n")
    );
}
