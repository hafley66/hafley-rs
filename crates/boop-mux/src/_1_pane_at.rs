//! Which pane of a session's active window a client cell lands on; its
//! `pane_current_path` is where the text under the pointer was printed from.

use std::path::PathBuf;

/// The pane under a client cell, and the cell translated into the pane's own
/// coordinates.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PaneHit {
    pub pane: String,
    pub pane_current_path: PathBuf,
    pub pane_col: u16,
    pub pane_row: u16,
}

/// The `list-panes` format `parse_pane_at` reads: one line per pane of the
/// target's active window, the status options repeated on every line.
pub(crate) const PANE_AT_FORMAT: &str = "#{pane_id}\t#{pane_left}\t#{pane_top}\t#{pane_width}\t#{pane_height}\t#{pane_active}\t#{window_zoomed_flag}\t#{status}\t#{status-position}\t#{pane_current_path}";

/// Rows the status line takes above the window. `status` is `off`, `on`, or a
/// line count `2`..`5`; only a top status shifts the window down.
fn status_rows_above(status: &str, position: &str) -> u16 {
    if position != "top" {
        return 0;
    }
    match status {
        "off" => 0,
        "on" => 1,
        count => count.parse().unwrap_or(1),
    }
}

/// Hit-test a client cell against `list-panes -F PANE_AT_FORMAT` output. A
/// zoomed window shows only its active pane; borders hit nothing.
pub fn parse_pane_at(text: &str, col: u16, row: u16) -> Option<PaneHit> {
    for line in text.lines() {
        let fields: Vec<&str> = line.splitn(10, '\t').collect();
        let [pane, left, top, width, height, active, zoomed, status, position, path] = fields[..] else {
            continue;
        };
        if zoomed == "1" && active != "1" {
            continue;
        }
        let (Ok(left), Ok(top), Ok(width), Ok(height)) =
            (left.parse::<u16>(), top.parse::<u16>(), width.parse::<u16>(), height.parse::<u16>())
        else {
            continue;
        };
        let Some(window_row) = row.checked_sub(status_rows_above(status, position)) else {
            return None;
        };
        let inside_col = col >= left && col < left.saturating_add(width);
        let inside_row = window_row >= top && window_row < top.saturating_add(height);
        if inside_col && inside_row {
            return Some(PaneHit {
                pane: pane.to_owned(),
                pane_current_path: PathBuf::from(path),
                pane_col: col - left,
                pane_row: window_row - top,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Two panes side by side (80 wide: 40 | border | 39) above one full-width
    // pane, status line at the bottom.
    const SPLIT: &str = "%1\t0\t0\t40\t12\t0\t0\ton\tbottom\t/repo/main\n\
%2\t41\t0\t39\t12\t1\t0\ton\tbottom\t/repo/wt\n\
%3\t0\t13\t80\t10\t0\t0\ton\tbottom\t/Users/me/projects/sqlite_ivm\n";

    #[test]
    fn a_cell_names_the_pane_it_lands_on() {
        let hits: Vec<Option<PaneHit>> = [(5, 3), (45, 0), (79, 11), (10, 20), (40, 3), (10, 12)]
            .into_iter()
            .map(|(col, row)| parse_pane_at(SPLIT, col, row))
            .collect();
        assert_eq!(
            hits,
            vec![
                Some(PaneHit { pane: "%1".into(), pane_current_path: "/repo/main".into(), pane_col: 5, pane_row: 3 }),
                Some(PaneHit { pane: "%2".into(), pane_current_path: "/repo/wt".into(), pane_col: 4, pane_row: 0 }),
                Some(PaneHit { pane: "%2".into(), pane_current_path: "/repo/wt".into(), pane_col: 38, pane_row: 11 }),
                Some(PaneHit {
                    pane: "%3".into(),
                    pane_current_path: "/Users/me/projects/sqlite_ivm".into(),
                    pane_col: 10,
                    pane_row: 7,
                }),
                // The vertical border column and the horizontal border row.
                None,
                None,
            ]
        );
    }

    #[test]
    fn a_top_status_line_shifts_the_window_down() {
        let top = "%1\t0\t0\t80\t23\t1\t0\ton\ttop\t/a\n";
        assert_eq!(parse_pane_at(top, 0, 0), None);
        assert_eq!(
            parse_pane_at(top, 0, 1),
            Some(PaneHit { pane: "%1".into(), pane_current_path: "/a".into(), pane_col: 0, pane_row: 0 })
        );
        let two = "%1\t0\t0\t80\t22\t1\t0\t2\ttop\t/a\n";
        assert_eq!(parse_pane_at(two, 3, 2).map(|hit| hit.pane_row), Some(0));
    }

    #[test]
    fn a_zoomed_window_hits_only_its_active_pane() {
        let zoomed = "%1\t0\t0\t40\t24\t0\t1\ton\tbottom\t/a\n%2\t0\t0\t80\t24\t1\t1\ton\tbottom\t/b\n";
        assert_eq!(parse_pane_at(zoomed, 5, 5).map(|hit| hit.pane), Some("%2".into()));
    }

    #[test]
    fn a_path_with_a_tab_survives_the_split() {
        let text = "%1\t0\t0\t80\t24\t1\t0\ton\tbottom\t/odd\tdir\n";
        assert_eq!(parse_pane_at(text, 0, 0).map(|hit| hit.pane_current_path), Some("/odd\tdir".into()));
    }
}
