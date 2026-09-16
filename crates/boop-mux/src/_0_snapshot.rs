//! Layer 0: the host-neutral terminal snapshot. One immutable grid plus the
//! size, screen, history and generation facts a renderer needs to place an
//! overlay on it. tmux is one provider of these; a direct PTY host is another,
//! and no tmux type appears below.
//!
//! `viewport_row` is a row index in this snapshot and nothing else: it is not a
//! stable identity, it is not history coordinates, and it changes meaning on
//! resize. Turn identity and pane identity live elsewhere.

use serde::{Deserialize, Serialize};
use unicode_width::UnicodeWidthStr;

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
/// `capture` is the whole visible grid top-down, one row per line, as a host
/// prints it. A row that fills the viewport width continues into the next one;
/// that is the only wrap evidence tmux exposes per row, so a capture must be
/// taken with trailing spaces preserved or a wrapped prose row reads short.
/// Rows the capture omits are trailing blank rows, which is where tmux pads.
pub fn rows_from_capture(capture: &str, size: TerminalSize) -> Vec<TerminalRow> {
    let mut lines: Vec<&str> = capture.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let mut rows: Vec<TerminalRow> = Vec::with_capacity(lines.len().max(size.rows as usize));
    let mut previous_filled = false;
    for line in lines {
        rows.push(TerminalRow {
            viewport_row: rows.len() as u16,
            text: line.trim_end().to_owned(),
            wraps_previous: previous_filled,
        });
        // A blank row never wraps: it is padding, not content that ran out of
        // columns.
        previous_filled = !line.trim().is_empty() && UnicodeWidthStr::width(line) >= size.columns as usize;
    }
    while (rows.len() as u16) < size.rows {
        rows.push(TerminalRow {
            viewport_row: rows.len() as u16,
            text: String::new(),
            wraps_previous: false,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(columns: u16, rows: u16) -> TerminalSize {
        TerminalSize { columns, rows }
    }

    #[test]
    fn a_row_that_fills_the_width_wraps_into_the_next() {
        let a = "A".repeat(10);
        let capture = format!("{a}\nBB\n");
        let rows = rows_from_capture(&capture, size(10, 12));
        assert!(!rows[0].wraps_previous);
        assert!(rows[1].wraps_previous, "a full-width row continues");
        assert!(!rows[2].wraps_previous, "a short row does not");
    }

    #[test]
    fn trailing_padding_is_not_read_as_content_that_wrapped() {
        // Trailing spaces from a `capture-pane -pN` full-width row are content;
        // an all-blank row of the same width is padding and must break the run.
        let blank = " ".repeat(10);
        let capture = format!("{blank}\nBB\n");
        let rows = rows_from_capture(&capture, size(10, 12));
        assert!(!rows[0].wraps_previous);
        assert!(!rows[1].wraps_previous);
    }

    #[test]
    fn wrapped_prose_keeps_its_wrap_when_the_row_ends_in_spaces() {
        let row = "word word ".to_owned() + &" ".repeat(2);
        let rows = rows_from_capture(&format!("{row}\nnext\n"), size(12, 12));
        assert_eq!(rows[0].text, "word word", "text is right-trimmed");
        assert!(rows[1].wraps_previous);
    }

    #[test]
    fn a_short_capture_is_padded_to_the_viewport_with_blanks() {
        let rows = rows_from_capture("only\n", size(40, 5));
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].viewport_row, 0);
        assert_eq!(rows[4].viewport_row, 4);
        assert_eq!(rows[4].text, "");
        assert!(!rows[4].wraps_previous);
    }

    #[test]
    fn an_empty_capture_is_a_blank_viewport() {
        let rows = rows_from_capture("", size(40, 3));
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| row.text.is_empty()));
    }

    #[test]
    fn wide_glyphs_are_measured_in_cells_not_chars() {
        // 6 CJK glyphs occupy 12 cells; the row fills a 12-column viewport.
        let rows = rows_from_capture("日本語日本語\nnext\n", size(12, 4));
        assert!(rows[1].wraps_previous);
    }

    #[test]
    fn grid_eq_ignores_generation_and_identity() {
        let rows = rows_from_capture("hello\n", size(20, 2));
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
