//! The lane screen: one terminal snapshot plus the squares a right-margin
//! navigator draws inside it.
//!
//! This is the join two crates apart cannot make on their own — `boop-mux`
//! reads the pane's grid, `boop-store` holds the turns, and the matcher decides
//! which turns those rows belong to. Nothing here writes: the snapshot is read,
//! the turns are read, the result is returned.

use boop_store::rows::TurnRow;
use boop_turnvis::{visible_squares_with, BoopTurn, TurnSquare};
use serde::Serialize;

use crate::tmux::TerminalSnapshot;

/// One screen read: where it came from, the grid itself, and the squares placed
/// on it. `snapshot.size` is the geometry the overlay is laid out against, so a
/// renderer never guesses the pane's shape.
#[derive(Clone, Debug, Serialize)]
pub struct ScreenState {
    pub lane: String,
    pub session: String,
    pub snapshot: TerminalSnapshot,
    pub squares: Vec<TurnSquare>,
}

/// The matcher's input shape, built from the store's turn rows. One field per
/// field: the row already carries the transcript's own vocabulary, and the
/// matcher's `turn` + `ts` are the reconnect coordinate it matches on.
pub fn turns_of(rows: &[TurnRow]) -> Vec<BoopTurn> {
    rows.iter()
        .map(|row| BoopTurn {
            session: row.session.clone(),
            harness: row.harness.clone(),
            turn: row.turn,
            ts: row.ts,
            role: row.role.clone(),
            said: row.said.clone(),
        })
        .collect()
}

/// Match a session's turns against one pane snapshot.
///
/// The caller scopes the rows: this does not filter by session, because the
/// matcher decides ownership from screen content, and a turn from elsewhere
/// whose text happens to be on the pane is not distinguishable here. `session`
/// is carried for identity, and the reads that feed it are already scoped.
pub fn screen_state(
    lane: &str,
    session: &str,
    snapshot: TerminalSnapshot,
    rows: &[TurnRow],
) -> ScreenState {
    ScreenState {
        lane: lane.to_owned(),
        session: session.to_owned(),
        squares: visible_squares_with(
            &snapshot,
            &turns_of(rows),
            Some(boop_harness::harness::claude_summary::anchor),
        ),
        snapshot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmux::{History, Screen, TerminalRow, TerminalSize, TerminalTarget};

    fn snapshot(rows: &[(&str, bool)]) -> TerminalSnapshot {
        TerminalSnapshot {
            target: TerminalTarget {
                host: "tmux".into(),
                terminal: "%4".into(),
                incarnation: 99,
            },
            generation: 0,
            size: TerminalSize {
                columns: 60,
                rows: rows.len() as u16,
            },
            screen: Screen::Primary,
            history: History::Retained {
                rows: 12,
                capacity: 2000,
            },
            cursor: Some((0, 0)),
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

    fn row(session: &str, turn: i64, role: &str, said: &str) -> TurnRow {
        TurnRow {
            session: session.into(),
            harness: "claude".into(),
            turn,
            ts: 1_700_000_000 + turn,
            role: role.into(),
            said: said.into(),
        }
    }

    #[test]
    fn a_screen_state_places_each_role_on_its_own_rows() {
        let state = screen_state(
            "claude-1",
            "session-a",
            snapshot(&[
                ("❯ hello from the human", false),
                ("", false),
                ("done", false),
            ]),
            &[
                row("session-a", 1, "user", "hello from the human"),
                row("session-a", 2, "assistant", "done"),
            ],
        );
        assert_eq!(state.lane, "claude-1");
        assert_eq!(state.session, "session-a");
        assert_eq!(state.snapshot.size.columns, 60);
        let sides: Vec<(&str, u16, u16)> = state
            .squares
            .iter()
            .map(|square| {
                (
                    square.role.as_str(),
                    square.viewport_start,
                    square.viewport_end,
                )
            })
            .collect();
        assert_eq!(sides, vec![("user", 0, 0), ("assistant", 2, 2)]);
    }

    #[test]
    fn a_turn_with_no_rows_on_screen_is_not_a_square() {
        let state = screen_state(
            "claude-1",
            "session-a",
            snapshot(&[("done", false)]),
            &[
                row("session-a", 1, "assistant", "done"),
                row(
                    "session-a",
                    2,
                    "assistant",
                    "a message that scrolled away entirely",
                ),
            ],
        );
        assert_eq!(
            state
                .squares
                .iter()
                .map(|square| square.turn)
                .collect::<Vec<i64>>(),
            vec![1]
        );
    }

    #[test]
    fn a_turn_that_is_not_on_screen_contributes_no_square() {
        // Matching is by screen content, so a turn whose text is nowhere on the
        // pane is dropped; the caller's query, not this join, does the scoping.
        let state = screen_state(
            "claude-1",
            "session-a",
            snapshot(&[("done", false)]),
            &[row("session-a", 1, "assistant", "something else entirely")],
        );
        assert!(state.squares.is_empty(), "{:#?}", state.squares);
    }
}
