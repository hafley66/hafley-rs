//! Layer 0: the host-neutral terminal snapshot. One immutable grid plus the
//! size, screen, history and generation facts a renderer needs to place an
//! overlay on it. tmux is one provider of these; a direct PTY host is another,
//! and no tmux type appears below.
//!
//! `viewport_row` is a row index in this snapshot and nothing else: it is not a
//! stable identity, it is not history coordinates, and it changes meaning on
//! resize. Turn identity and pane identity live elsewhere.

use serde::{Deserialize, Serialize};

/// The terminal-state instance a snapshot came from. `incarnation` changes when
/// the host replaces its terminal state, so a stale snapshot never joins a new
/// one. For tmux the incarnation is the server pid: a restarted server reissues
/// the same pane ids and every captured row means something else.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalTarget {
    pub host: String,
    pub terminal: String,
    pub incarnation: u64,
}

/// Viewport geometry in cells. This is the one input a renderer may not guess:
/// every right-margin overlay, column split and row window is arithmetic on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSize {
    pub columns: u16,
    pub rows: u16,
}

/// Which screen the rows came from. An alternate screen has no retained
/// history; a renderer that scrolls back is scrolling into a full-screen
/// program's own redraw, not into a log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Screen {
    Primary,
    Alternate,
}

/// How much history the host can hand back beyond the visible grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum History {
    Retained { rows: u32, capacity: u32 },
    Unavailable,
}

/// One visible row. `text` is right-trimmed; `wraps_previous` says the row
/// continues the one above it, so a logical line can be rebuilt without
/// guessing from text content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalRow {
    pub viewport_row: u16,
    pub text: String,
    pub wraps_previous: bool,
}

/// One immutable grid snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSnapshot {
    pub target: TerminalTarget,
    /// Assigned by the source. `0` when the source is a one-shot probe with no
    /// memory of its last answer; such a source compares with `grid_eq`.
    pub generation: u64,
    pub size: TerminalSize,
    pub screen: Screen,
    pub history: History,
    pub cursor: Option<(u16, u16)>,
    pub rows: Vec<TerminalRow>,
}

impl TerminalSnapshot {
    /// Whether two snapshots carry the same grid, size, screen, history and
    /// cursor. The change detector for a source that cannot number its
    /// generations: equal means a render built on either is still correct.
    pub fn grid_eq(&self, other: &TerminalSnapshot) -> bool {
        self.size == other.size
            && self.screen == other.screen
            && self.history == other.history
            && self.cursor == other.cursor
            && self.rows == other.rows
    }
}

/// Rebuild the visible rows of one capture into viewport rows.
///
/// `capture` is `capture-pane -p`: one line per visible row, each right-trimmed
/// by tmux. `joined` is `capture-pane -pJ` over the same grid, where tmux has
/// already applied its own per-row wrap flag.
///
/// Width cannot carry that flag. The padded capture (`-N`) pads every row the
/// cursor has left to the full pane width, so a full-width row is the ordinary
/// case and the flag would fire on every row; and a right-trimmed capture loses
/// the wrap of a row that broke after a space. Tmux exposes no per-row wrap
/// predicate, so the two views are aligned by their whitespace-stripped
/// content, which is exact because they are two readings of one grid.
///
/// Rows the capture omits are trailing blank rows, which is where tmux pads.
pub fn rows_from_capture(capture: &str, joined: &str, size: TerminalSize) -> Vec<TerminalRow> {
    let physical = capture_rows(capture);
    let logical: Vec<String> = capture_rows(joined).into_iter().map(strip_ws).collect();
    let mut wraps = vec![false; physical.len()];
    let mut index = 0;
    for target in &logical {
        if index >= physical.len() {
            break;
        }
        let start = index;
        let mut accumulated = strip_ws(physical[index]);
        index += 1;
        while accumulated != *target && index < physical.len() {
            let next = strip_ws(physical[index]);
            if accumulated.len() + next.len() > target.len() {
                // The row belongs to the next logical line; leaving it in place
                // keeps the walk aligned for every line after it.
                break;
            }
            accumulated.push_str(&next);
            index += 1;
        }
        for row in start + 1..index {
            wraps[row] = true;
        }
    }
    let mut rows: Vec<TerminalRow> = physical
        .iter()
        .enumerate()
        .map(|(viewport_row, text)| TerminalRow {
            viewport_row: viewport_row as u16,
            text: text.trim_end().to_owned(),
            wraps_previous: wraps[viewport_row],
        })
        .collect();
    while (rows.len() as u16) < size.rows {
        rows.push(TerminalRow {
            viewport_row: rows.len() as u16,
            text: String::new(),
            wraps_previous: false,
        });
    }
    rows
}

/// The rows of one capture, with tmux's single trailing newline dropped.
fn capture_rows(capture: &str) -> Vec<&str> {
    let mut rows: Vec<&str> = capture.split('\n').collect();
    if rows.last() == Some(&"") {
        rows.pop();
    }
    rows
}

/// Content without any whitespace: what makes the trimmed and the joined view
/// of one grid comparable at a row boundary.
fn strip_ws(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(columns: u16, rows: u16) -> TerminalSize {
        TerminalSize { columns, rows }
    }

    #[test]
    fn a_row_tmux_joined_to_the_one_above_it_wraps() {
        let rows = rows_from_capture(
            "alpha beta gamma delta epsilon\nzeta eta theta\n",
            "alpha beta gamma delta epsilon zeta eta theta\n",
            size(60, 4),
        );
        assert!(!rows[0].wraps_previous);
        assert!(rows[1].wraps_previous, "tmux joined it to the row above");
        assert!(!rows[2].wraps_previous);
        assert_eq!(rows[0].text, "alpha beta gamma delta epsilon");
    }

    #[test]
    fn a_logical_line_boundary_is_not_a_wrap() {
        // The regression: a padded capture (`-pN`) reports every row the cursor
        // has left at the full pane width, so a width test flags an ordinary
        // row, joins a message to the row under it, and moves the turn span.
        let capture = "❯ how could we nest things\n\nRail agent finished green\n";
        let rows = rows_from_capture(capture, capture, size(200, 5));
        assert!(
            rows.iter().all(|row| !row.wraps_previous),
            "three separate rows, none joined: {rows:#?}"
        );
        assert_eq!(rows[1].text, "");
    }

    #[test]
    fn a_wrapped_row_that_ends_in_spaces_still_joins() {
        // The trimmed capture keeps no trailing space, so the only evidence is
        // tmux's own join on the same grid.
        let rows = rows_from_capture(
            "alpha beta gamma delta epsilon zeta\nzeta eta theta\n",
            "alpha beta gamma delta epsilon zeta zeta eta theta\n",
            size(40, 4),
        );
        assert!(rows[1].wraps_previous);
    }

    #[test]
    fn a_short_capture_is_padded_to_the_viewport_with_blanks() {
        let rows = rows_from_capture("only\n", "only\n", size(40, 5));
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].viewport_row, 0);
        assert_eq!(rows[4].viewport_row, 4);
        assert_eq!(rows[4].text, "");
        assert!(!rows[4].wraps_previous);
    }

    #[test]
    fn an_empty_capture_is_a_blank_viewport() {
        let rows = rows_from_capture("", "", size(40, 3));
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| row.text.is_empty()));
    }

    #[test]
    fn wide_glyphs_join_like_any_other_content() {
        let rows = rows_from_capture(
            "日本語日本語\nテスト\n",
            "日本語日本語テスト\n",
            size(12, 4),
        );
        assert!(rows[1].wraps_previous);
    }

    #[test]
    fn grid_eq_ignores_generation_and_identity() {
        let rows = rows_from_capture("hello\n", "hello\n", size(20, 2));
        let base = TerminalSnapshot {
            target: TerminalTarget {
                host: "tmux".into(),
                terminal: "%1".into(),
                incarnation: 7,
            },
            generation: 3,
            size: size(20, 2),
            screen: Screen::Primary,
            history: History::Retained {
                rows: 4,
                capacity: 2000,
            },
            cursor: Some((0, 1)),
            rows: rows.clone(),
        };
        let mut later = base.clone();
        later.generation = 91;
        later.target.incarnation = 9;
        assert!(base.grid_eq(&later));
        later.cursor = Some((3, 1));
        assert!(!base.grid_eq(&later));
    }
}
