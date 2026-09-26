// Rows copied from a live claude pane (2026-09-26): a reply claude hard-wrapped
// across two rows, an old turn whose text also contains the second row, and two
// expanded tool blocks, each its own assistant turn as boop stores them.
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
    }
}

#[test]
fn wrapped_reply_keeps_its_rows_and_tool_blocks_anchor() {
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
        // `cap(input, 400)` cuts long inputs mid-string, so the stored JSON need not parse.
        turn(522, "[Write] {\"content\":\"# Plan: TypeSpec-first UI names\\n\\nStatus: draft.\",\"file_path\":\"/Users/chrishafley/projects/hafley-rxjs/plans/2026-09-26-tsp-ui-names.PLAN.md\""),
    ];
    let found: Vec<String> = locate_visible_turns(&screen, &turns)
        .iter()
        .map(|t| format!("{} anchor {}..{} buffer {}..{} {:?}", t.turn, t.anchor_start, t.anchor_end, t.buffer_start, t.buffer_end, t.confidence))
        .collect();
    assert_eq!(
        found.join("\n"),
        [
            "518 anchor 0..2 buffer 0..2 Anchored",
            "520 anchor 4..4 buffer 4..5 Extended",
            "522 anchor 7..7 buffer 7..8 Extended",
        ]
        .join("\n")
    );
}
