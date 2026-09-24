//! The terminal turn strip: where each turn's square goes, from the pane's rows
//! and the turns that landed on them.
//!
//! The chain, and where this crate sits in it:
//!
//! ```text
//!   grid rows + BoopTurn[]        boop / xterm
//!     -> VisibleTurn[]            boop-turnvis, the matcher: spans + ids
//!     -> TurnRow[]                rows_of: rows and lines in the viewport
//!     -> Estimates + Placement[]  measure, place_window
//!     -> Layout                   strip_layout: y, scale, the active square
//! ```
//!
//! Nothing here touches IO or a UI. `boop-turnvis` maps the pane's rows to the
//! turns on them; this crate is a pure function of that projection plus a
//! viewport, so the strip moves on a scroll without a single query. A caller
//! runs it wherever it happens to live — a Rust server drawing the strip,
//! a CLI reporting it, or a wasm binding driving the TypeScript client — and
//! hands the result over as JSON, because every wire shape is camelCase and
//! `TurnKind` is lowercase.
//!
//! The one entry point is [`layout`]:
//!
//! ```no_run
//! use boop_turnstrip::{layout, rows_of, Layout, Options, TurnRow, Viewport};
//! use boop_turnvis::{locate_visible_turns, BoopTurn, LogicalLine};
//!
//! /// The strip for one pane: what the matcher sees, measured and placed.
//! fn strip(
//!     rows: &[LogicalLine],
//!     turns: &[BoopTurn],
//!     viewport: Viewport,
//!     focus_row: i64,
//! ) -> Layout {
//!     let visible = locate_visible_turns(rows, turns);
//!     let measured: Vec<TurnRow> = visible
//!         .iter()
//!         .map(|turn| rows_of(rows, turn, viewport))
//!         .collect();
//!     layout(&measured, viewport, focus_row, &Options::default())
//! }
//! ```
//!
//! The pieces are public too, for a caller that keeps its own pipeline:
//! [`samples_from`] and [`measure`] measure the viewport, [`estimate_rows`]
//! extrapolates one turn, [`place_window`] places the window, and
//! [`align_rows`] is the source-line-to-screen-row aligner underneath
//! [`rows_of`].

mod _0_types;
mod _1_measure;
mod _2_place;
mod _3_layout;
mod _3a_gap;

pub use _0_types::{
    clamp, kind_of, lines_of, Estimates, Layout, ListedTurn, Mode, Options, Placement, RecentStrip,
    RelativeStrip, Square, ToolGap, TurnKind, TurnRow, TurnSample, Viewport, WindowTurn,
    DEFAULT_RECENT_MAX, KINDS, STRIP_DEFAULTS, ZEROED,
};
pub use _1_measure::{align_rows, estimate_rows, measure, rows_of, samples_from};
pub use _2_place::{place_window, window_of};
pub use _3_layout::{
    drawn_as_tool, drawn_at_all, layout, layout_pinned, recent_layout, relative_layout,
};

#[cfg(test)]
mod tests {
    use super::*;
    use boop_turnvis::{Confidence, LogicalLine, VisibleTurn};

    /// The TypeScript fixture pins its numbers to two decimals
    /// (`Number(value.toFixed(2))`); a match here is a value that rounds to the
    /// same hundredth.
    fn close(got: f64, want: f64) -> bool {
        (got - want).abs() <= 0.005 + 1e-9
    }

    fn visible_turn(id: &str, role: &str, span: (usize, usize), total: usize) -> VisibleTurn {
        let said: Vec<String> = (1..=total).map(|index| format!("line {index}")).collect();
        VisibleTurn {
            session: "s1".to_string(),
            harness: "claude".to_string(),
            turn: 1,
            ts: 0,
            role: role.to_string(),
            said: said.join("\n"),
            id: id.to_string(),
            buffer_start: span.0,
            buffer_end: span.1,
            anchor_start: span.0,
            anchor_end: span.1,
            confidence: Confidence::Anchored,
        }
    }

    fn grid(rows: &[(usize, String)]) -> Vec<LogicalLine> {
        rows.iter()
            .map(|(start, text)| LogicalLine {
                text: text.clone(),
                start: *start,
                end: *start,
            })
            .collect()
    }

    #[test]
    fn kind_of_maps_a_role_to_a_square_kind() {
        assert_eq!(kind_of("user"), TurnKind::User);
        assert_eq!(kind_of("tool"), TurnKind::Tool);
        assert_eq!(kind_of("assistant"), TurnKind::Agent);
        assert_eq!(kind_of("system"), TurnKind::Other);
        // Case matters: the harness's own word, not a guess.
        assert_eq!(kind_of("User"), TurnKind::Other);
    }

    #[test]
    fn lines_of_counts_newlines_and_floors_at_one() {
        assert_eq!(lines_of(""), 1);
        assert_eq!(lines_of("one"), 1);
        assert_eq!(lines_of("one\ntwo"), 2);
        // A trailing newline is an empty last line, as `split("\n")` counts it.
        assert_eq!(lines_of("one\n"), 2);
    }

    #[test]
    fn displayed_line_counts_exclude_only_boop_envelopes() {
        let screen = grid(&[(0, "actual user content".into()), (1, "second line".into())]);
        let mut turn = visible_turn("s1:1", "user", (0, 1), 3);
        turn.said = "[boop m1 from coordinator]\nactual user content\nsecond line".into();
        let measured = rows_of(&screen, &turn, Viewport { top: 0, bottom: 1 });
        assert_eq!((measured.total, measured.lines), (2, 2));
        assert_eq!(turn.said.lines().count(), 3);
        turn.said = "[ordinary brackets]\nactual user content\nsecond line".into();
        let measured = rows_of(&screen, &turn, Viewport { top: 0, bottom: 1 });
        assert_eq!((measured.total, measured.lines), (3, 2));
    }

    #[test]
    fn align_rows_matches_only_the_aligned_lines_on_a_shuffled_screen() {
        let source: Vec<String> = ["let total = 1", "print(total)", "done"]
            .iter()
            .map(|line| line.to_string())
            .collect();
        let screen = grid(&[
            (0, "noise".to_string()),
            (1, "let total = 1".to_string()),
            (2, "other junk".to_string()),
            (3, "print(total)".to_string()),
            (4, "trailing".to_string()),
            (5, "done".to_string()),
        ]);
        assert_eq!(align_rows(&source, &screen), vec![(0, 1), (1, 3), (2, 5)]);
    }

    #[test]
    fn align_rows_scores_by_utf16_code_units_not_scalars() {
        // Four astral characters are four `char`s — under the aligner's
        // eight-code-unit containment bar — but eight UTF-16 units, which is
        // what the TypeScript `.length` scores, so this pair does align.
        let source = vec!["😀😀😀😀".to_string()];
        let screen = grid(&[(4, "px 😀😀😀😀 px".to_string())]);
        assert_eq!(align_rows(&source, &screen), vec![(0, 4)]);
    }

    #[test]
    fn estimate_rows_clamps_to_the_lines_and_to_the_worst_wrap() {
        let turn = WindowTurn {
            id: "w".to_string(),
            kind: TurnKind::Agent,
            total: 4,
        };
        let estimates = |kappa: f64, kappa_max: f64| Estimates {
            kappa: [kappa; 4],
            gamma: ZEROED,
            kappa_max,
        };
        // 0.5 × 4 = 2 rows, but a turn cannot be shorter than its own lines.
        assert!(close(estimate_rows(&turn, &estimates(0.5, 2.0)), 4.0));
        // 3 × 4 = 12 rows, but no worse than the worst wrap the window measured.
        assert!(close(estimate_rows(&turn, &estimates(3.0, 2.0)), 8.0));
        assert!(close(estimate_rows(&turn, &estimates(1.5, 2.0)), 6.0));
    }

    #[test]
    fn layout_of_no_rows_is_an_empty_strip() {
        let options = Options::default();
        let strip = layout(&[], Viewport { top: 0, bottom: 23 }, 0, &options);
        assert!(strip.squares().is_empty());
        assert_eq!(strip.band(), 0);
    }

    #[test]
    fn rows_of_counts_a_half_visible_turns_lines_as_its_visible_share() {
        let turn = visible_turn("h1", "assistant", (5, 14), 10);
        let screen = grid(
            &(5..=14)
                .enumerate()
                .map(|(offset, row)| (row, format!("line {}", offset + 1)))
                .collect::<Vec<_>>(),
        );
        let row = rows_of(
            &screen,
            &turn,
            Viewport {
                top: 10,
                bottom: 20,
            },
        );
        assert_eq!((row.lines, row.total), (5, 10));
        assert_eq!((row.start, row.end), (5, 14));
        assert_eq!(row.kind, TurnKind::Agent);
    }

    #[test]
    fn rows_of_never_returns_zero_lines() {
        let turn = visible_turn("h1", "assistant", (5, 14), 10);
        let screen = grid(
            &(5..=14)
                .enumerate()
                .map(|(offset, row)| (row, format!("line {}", offset + 1)))
                .collect::<Vec<_>>(),
        );
        // The viewport is below every row the turn has: nothing of it showed.
        let row = rows_of(
            &screen,
            &turn,
            Viewport {
                top: 30,
                bottom: 40,
            },
        );
        assert_eq!(row.lines, 1);
        assert_eq!(row.total, 10);
    }
}
