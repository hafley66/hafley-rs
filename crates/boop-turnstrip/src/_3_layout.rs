//! The strip itself: one uniform block per square, its scale flexed by the
//! turn's share of the window and clamped, its position taken from the
//! estimated rows so a heavy tool turn occupies more of the strip than a
//! one-line prompt.
//!
//! `layout` is the whole pipeline in one call — rows to samples, samples to
//! `kappa`/`gamma`, placements, strip — and it is the entry point a server,
//! a CLI or a wasm binding uses.

use crate::_0_types::{clamp, Block, Layout, Options, Placement, Square, TurnRow, Viewport};
use crate::_1_measure::{measure, samples_from};
use crate::_2_place::{place_window, window_of};

/// A buffer row to the strip's track, in the same `0..track` space the squares
/// use. Linear in buffer rows within a turn and continuous across the seams,
/// which is what lets anything else be drawn in the strip's space.
fn row_at(placements: &[Placement], span: f64, track: f64, row: f64) -> f64 {
    let mut behind = 0.0f64;
    for placement in placements {
        if row < placement.start {
            break;
        }
        if row <= placement.start + placement.rows - 1.0 {
            return ((behind + (row - placement.start)) / span) * track;
        }
        behind += placement.rows;
    }
    (behind / span) * track
}

/// The strip: one height, a clamped scale, and the block the reader is in.
///
/// `focus_row` is the row the reader has at the top of the pane; `None` means
/// nothing is focused, and the newest square is marked instead. `layout`
/// always has a row, so it passes `Some`.
pub fn strip_layout(
    placements: &[Placement],
    focus_row: Option<i64>,
    viewport: Viewport,
    options: &Options,
) -> Layout {
    let total: f64 = placements.iter().map(|placement| placement.rows).sum();
    let span = if total == 0.0 { 1.0 } else { total };
    let track = 0.0f64.max(options.strip_max - options.square_height);

    // The window's median turn is the reference size: it keeps the plain
    // square while bigger turns grow and smaller ones shrink, both clamped.
    let reference = {
        let mut totals: Vec<f64> = placements
            .iter()
            .map(|placement| placement.total as f64)
            .collect();
        totals.sort_by(f64::total_cmp);
        if totals.is_empty() {
            1.0
        } else {
            let middle = totals.len() >> 1;
            if totals.len() % 2 == 1 {
                totals[middle]
            } else {
                (totals[middle - 1] + totals[middle]) / 2.0
            }
        }
    };

    let active = match focus_row {
        None => placements.len() as i64 - 1,
        Some(_) if placements.is_empty() => placements.len() as i64 - 1,
        Some(row) => {
            let row = row as f64;
            match placements.iter().position(|placement| {
                row >= placement.start && row <= placement.start + placement.rows - 1.0
            }) {
                Some(index) => index as i64,
                // A row past the last square, or above the first, falls back to
                // the end the reader is heading for.
                None if row < placements[0].start => 0,
                None => placements.len() as i64 - 1,
            }
        }
    };

    let squares: Vec<Square> = placements
        .iter()
        .enumerate()
        .map(|(index, placement)| Square {
            id: placement.id.clone(),
            kind: placement.kind,
            y: row_at(placements, span, track, placement.start),
            scale: clamp(
                1.0 + options.ratio_flex * (placement.total as f64 / 1.0f64.max(reference) - 1.0),
                options.scale_min,
                options.scale_max,
            ),
            active: index as i64 == active,
        })
        .collect();

    let top = row_at(placements, span, track, viewport.top as f64);
    let height = options
        .block_min
        .max(row_at(placements, span, track, viewport.bottom as f64 + 1.0) - top);
    Layout {
        squares,
        span,
        block: Block { top, height },
    }
}

/// Rows and a viewport in, a strip out.
///
/// The strip has its own rendering budget: `options.max_squares` keeps only the
/// newest rows of the window (`0` means no cap). The window is ordered by
/// buffer row, so the newest are its tail. The trim happens before anything is
/// measured, so a cap changes `span` and with it every `y` — a caller that
/// lowers it gets a genuinely different strip, not a truncated one.
pub fn layout(rows: &[TurnRow], viewport: Viewport, focus_row: i64, options: &Options) -> Layout {
    let capped: Option<Vec<TurnRow>> =
        (options.max_squares > 0 && rows.len() > options.max_squares).then(|| {
            let mut ordered = rows.to_vec();
            ordered.sort_by(|left, right| left.start.cmp(&right.start));
            ordered.split_off(ordered.len() - options.max_squares)
        });
    let kept: &[TurnRow] = capped.as_deref().unwrap_or(rows);

    let samples = samples_from(kept, viewport);
    let estimates = measure(&samples, viewport);
    let window = window_of(kept);
    let placements = place_window(&window, &samples, &estimates, viewport);
    strip_layout(&placements, Some(focus_row), viewport, options)
}
