//! Snapshot input: the adapter from a terminal snapshot to the matcher's
//! logical lines, and the navigator squares a right-margin overlay draws.
//!
//! `lib.rs` holds the matcher and is the port of the TypeScript version; it
//! takes logical lines and knows nothing about viewports. This module is the
//! seam above it: a snapshot's rows become logical lines, and the matched turns
//! become the compact per-turn squares a UI places in the margin.

use boop_mux::TerminalSnapshot;
use serde::{Deserialize, Serialize};

use crate::{
    locate_visible_turns_with, BoopTurn, Confidence, LogicalLine, SummaryAnchor, VisibleTurn,
};

/// How much of a turn's text a hover preview carries. Matches the 120-character
/// preview Instant's mail events already expose, so the navigator and the
/// mailbox describe a turn the same way.
pub const PREVIEW_CHARS: usize = 120;

/// Rebuild the matcher's logical lines from a snapshot's viewport rows. A row
/// flagged `wraps_previous` continues the line above it instead of starting a
/// new one, so a wrapped message matches as one line rather than as fragments.
pub fn logical_lines(snapshot: &TerminalSnapshot) -> Vec<LogicalLine> {
    let mut lines: Vec<LogicalLine> = Vec::with_capacity(snapshot.rows.len());
    for row in &snapshot.rows {
        let viewport_row = row.viewport_row as usize;
        match lines.last_mut() {
            Some(line) if row.wraps_previous => {
                line.text.push_str(&row.text);
                line.end = viewport_row;
            }
            _ => lines.push(LogicalLine {
                text: row.text.clone(),
                start: viewport_row,
                end: viewport_row,
            }),
        }
    }
    lines
}

/// Match store turns against a snapshot's grid. The viewport-row spans on the
/// result are rows in this snapshot, so they go stale on resize; turn identity
/// is `id`, which is `"<session>:<turn>"`.
pub fn locate_snapshot_turns(snapshot: &TerminalSnapshot, turns: &[BoopTurn]) -> Vec<VisibleTurn> {
    locate_snapshot_turns_with(snapshot, turns, None)
}

/// Match a snapshot with a harness adapter's transcript-shape hook.
pub fn locate_snapshot_turns_with(
    snapshot: &TerminalSnapshot,
    turns: &[BoopTurn],
    summary_anchor: Option<SummaryAnchor>,
) -> Vec<VisibleTurn> {
    locate_visible_turns_with(&logical_lines(snapshot), turns, summary_anchor)
}

/// One square in the right-margin navigator: a turn whose rows are on screen,
/// placed by viewport row. `role` picks the side; the spans are the rows the
/// overlay anchors to, not the rows the turn owns in the transcript.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnSquare {
    pub id: String,
    pub session: String,
    pub harness: String,
    pub turn: i64,
    pub ts: i64,
    pub role: String,
    pub viewport_start: u16,
    pub viewport_end: u16,
    pub preview: String,
    pub confidence: Confidence,
}

/// The squares for one snapshot: every visible turn, in screen order, carrying
/// identity, role, timestamps and a preview but none of the message body.
pub fn visible_squares(snapshot: &TerminalSnapshot, turns: &[BoopTurn]) -> Vec<TurnSquare> {
    visible_squares_with(snapshot, turns, None)
}

/// Build squares with a harness adapter's transcript-shape hook.
pub fn visible_squares_with(
    snapshot: &TerminalSnapshot,
    turns: &[BoopTurn],
    summary_anchor: Option<SummaryAnchor>,
) -> Vec<TurnSquare> {
    locate_snapshot_turns_with(snapshot, turns, summary_anchor)
        .into_iter()
        .map(|turn| TurnSquare {
            id: turn.id,
            session: turn.session,
            harness: turn.harness,
            turn: turn.turn,
            ts: turn.ts,
            role: turn.role,
            viewport_start: turn.buffer_start as u16,
            viewport_end: turn.buffer_end as u16,
            preview: turn.said.chars().take(PREVIEW_CHARS).collect(),
            confidence: turn.confidence,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use boop_mux::{History, Screen, TerminalRow, TerminalSize, TerminalTarget};

    fn snapshot(rows: &[(&str, bool)]) -> TerminalSnapshot {
        TerminalSnapshot {
            target: TerminalTarget {
                host: "test".into(),
                terminal: "%1".into(),
                incarnation: 1,
            },
            generation: 0,
            size: TerminalSize {
                columns: 40,
                rows: rows.len() as u16,
            },
            screen: Screen::Primary,
            history: History::Unavailable,
            cursor: None,
            rows: rows
                .iter()
                .enumerate()
                .map(|(index, (text, wraps_previous))| TerminalRow {
                    viewport_row: index as u16,
                    text: (*text).to_owned(),
                    wraps_previous: *wraps_previous,
                })
                .collect(),
        }
    }

    fn turn(turn: i64, role: &str, said: &str) -> BoopTurn {
        BoopTurn {
            session: "s".into(),
            harness: "claude".into(),
            turn,
            ts: 1_700_000_000 + turn,
            role: role.into(),
            said: said.into(),
        }
    }

    #[test]
    fn a_wrapped_row_joins_the_line_above_it() {
        let lines = logical_lines(&snapshot(&[
            ("alpha beta", false),
            ("gamma delta", true),
            ("omega", false),
        ]));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "alpha betagamma delta");
        assert_eq!((lines[0].start, lines[0].end), (0, 1));
        assert_eq!(lines[1].text, "omega");
        assert_eq!((lines[1].start, lines[1].end), (2, 2));
    }

    #[test]
    fn squares_carry_the_role_the_span_and_a_preview() {
        let screen = snapshot(&[
            ("❯ hello from the human", false),
            ("", false),
            ("done", false),
        ]);
        let squares = visible_squares(
            &screen,
            &[
                turn(1, "user", "hello from the human"),
                turn(2, "assistant", "done"),
            ],
        );
        assert_eq!(squares.len(), 2, "{squares:#?}");
        assert_eq!(squares[0].role, "user");
        assert_eq!(squares[0].id, "s:1");
        assert_eq!(squares[0].ts, 1_700_000_001);
        assert_eq!((squares[0].viewport_start, squares[0].viewport_end), (0, 0));
        assert_eq!(squares[1].role, "assistant");
        assert_eq!((squares[1].viewport_start, squares[1].viewport_end), (2, 2));
        assert_eq!(squares[1].preview, "done");
    }

    #[test]
    fn a_preview_is_capped_and_counts_characters_not_bytes() {
        // The screen holds a prefix of the turn, which is the only shape the
        // matcher's containment rule accepts for a source longer than the row.
        let row = "λ".repeat(20);
        let said = "λ".repeat(PREVIEW_CHARS + 40);
        let squares = visible_squares(&snapshot(&[(row.as_str(), false)]), &[turn(1, "user", &said)]);
        assert_eq!(squares.len(), 1, "{squares:#?}");
        assert_eq!(squares[0].preview, "λ".repeat(PREVIEW_CHARS));
    }
}
