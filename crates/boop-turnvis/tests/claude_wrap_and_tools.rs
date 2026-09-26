// Rows copied from a live claude pane (2026-09-26): a reply claude hard-wrapped across
// two rows, and an old turn whose text also contains the second row.
use boop_turnvis::{locate_visible_turns, BoopTurn, LogicalLine};

fn line(row: usize, text: &str) -> LogicalLine {
    LogicalLine { text: text.to_owned(), start: row, end: row }
}

fn turn(turn: i64, said: &str) -> BoopTurn {
    BoopTurn {
        session: "s".to_owned(),
        harness: "claude".to_owned(),
        turn,
        ts: turn,
        role: "assistant".to_owned(),
        said: said.to_owned(),
        aliases: Vec::new(),
    }
}

#[test]
fn wrapped_reply_keeps_its_rows() {
    let screen = [
        line(0, "⏺ The test-budget lane has retired. Its worktree is clean, and one message to it would bring back the same"),
        line(1, "  conversation. Nothing is running now; the two items above are still"),
        line(2, "  waiting on you."),
    ];
    let turns = [
        turn(11, "**Done**\n- Waves 1 and 2 are merged in instant and hafley-rxjs.\n\n**Not done**\n- 6 decisions waiting on you."),
        turn(518, "The test-budget lane has retired. Its worktree is clean, and one message to it would bring back the same conversation. Nothing is running now; the two items above are still waiting on you."),
    ];
    let found: Vec<String> = locate_visible_turns(&screen, &turns)
        .iter()
        .map(|t| format!("{} anchor {}..{} buffer {}..{} {:?}", t.turn, t.anchor_start, t.anchor_end, t.buffer_start, t.buffer_end, t.confidence))
        .collect();
    assert_eq!(found.join("\n"), "518 anchor 0..2 buffer 0..2 Anchored");
}
