//! The strip itself, in the two modes that answer "where does a square go".
//!
//!   - [`Mode::Relative`]: the strip is the reader's window. A square sits on
//!     the row its turn starts on, which is what makes the strip and the
//!     terminal agree — a caller that knows its cell height draws
//!     `y * cell_height` and the square lands on the turn's own row. A scroll
//!     moves the window, so it moves every `y` by the same amount with nothing
//!     else sent. Only a turn the matcher saw on these rows has a row to name:
//!     an estimate is a guess at where a turn *would* be, and a square drawn
//!     from one points at text that is not its own, which is exactly what this
//!     mode must not do. The turn the reader's top row is inside is always one
//!     of the squares, whatever the gap or the cap says.
//!   - [`Mode::Map`]: the strip is the rolling window of turns, every one of
//!     them placed in estimated buffer rows, so a heavy tool turn takes more of
//!     the map than a one-line prompt and a turn above the window still draws.
//!     A scroll moves the block that marks the reader's rows and leaves the
//!     squares alone.
//!
//! Both modes keep the reader's own turns on the strip even when they place
//! none of them: the *band*, a short run of pinned squares at the head of the
//! layout. Those are the turns a reader navigates by — the prompts they wrote —
//! so losing them to a scroll is worse than drawing them off-position. They are
//! not positions and carry no row: `y` counts places in the band.
//!
//! `layout` is the whole pipeline in one call — rows to samples, samples to
//! `kappa`/`gamma`, placements, strip — and it is the entry point a server,
//! a CLI or a wasm binding uses.

use crate::_0_types::{
    clamp, Block, Layout, MapStrip, Mode, Options, Placement, RelativeStrip, Square, TurnKind,
    TurnRow, Viewport,
};
use crate::_1_measure::{measure, samples_from};
use crate::_2_place::{place_window, window_of};

/// The reader's own turns the strip would otherwise place nothing for: the
/// newest `options.user_keep` of `pins` that no square already carries, oldest
/// first, as band squares.
///
/// `pins` arrives oldest first — the caller read the session's turns and kept
/// the ones the matcher did not find on screen — so the tail is the newest,
/// which is the end a reader wants kept when the band is too short.
fn band(pins: &[String], drawn: &[String], options: &Options) -> Vec<Square> {
    if options.user_keep == 0 {
        return Vec::new();
    }
    let mut kept: Vec<&String> = pins
        .iter()
        .filter(|id| !drawn.iter().any(|seen| seen == *id))
        .collect();
    if kept.len() > options.user_keep {
        kept.drain(..kept.len() - options.user_keep);
    }
    kept.iter()
        .enumerate()
        .map(|(index, id)| Square {
            id: (*id).clone(),
            kind: TurnKind::User,
            y: index as f64,
            scale: 1.0,
            active: false,
        })
        .collect()
}

/// A turn the strip draws at all. The tool filter lives here so a hidden turn
/// costs no square anywhere: not a place in the band, not a slot in the
/// spacing pass, not a row of the map.
fn drawn_at_all(kind: TurnKind, options: &Options) -> bool {
    options.show_tools || kind != TurnKind::Tool
}

/// The rows a placement occupies, in buffer rows: what the matcher saw of it
/// for a measured turn, its estimate otherwise.
fn extent(placement: &Placement) -> (f64, f64) {
    match placement.visible {
        Some((start, end)) => (start, end),
        None => (placement.start, placement.start + placement.rows - 1.0),
    }
}

/// Where a turn's square sits, in window rows, or `None` when the turn is not
/// in the window at all.
///
/// The anchor is the turn's *first visible* row: a turn whose head scrolled off
/// the top draws at row 0 rather than off the strip, because the reader is
/// inside that turn and the square has to say so. The rows are the ones the
/// matcher attributed — `placement.visible` — and not the placement's estimate
/// of the whole turn: an estimate is a guess at where a turn *would* be, and a
/// square drawn from one points at text that is not its own, which is exactly
/// what the strip must not do.
fn window_row(placement: &Placement, top: f64, bottom: f64) -> Option<f64> {
    let (start, end) = placement.visible?;
    if !start.is_finite() || !end.is_finite() || end < start {
        return None;
    }
    if end < top || start > bottom {
        return None;
    }
    Some(start.max(top) - top)
}

/// The window's median turn, the reference size: it keeps the plain square
/// while bigger turns grow and smaller ones shrink, both clamped.
fn reference_of(placements: &[Placement], kept: &[(usize, f64)]) -> f64 {
    let mut totals: Vec<f64> = kept
        .iter()
        .map(|&(index, _)| placements[index].total as f64)
        .collect();
    totals.sort_by(f64::total_cmp);
    if totals.is_empty() {
        return 1.0;
    }
    let middle = totals.len() >> 1;
    if totals.len() % 2 == 1 {
        totals[middle]
    } else {
        (totals[middle - 1] + totals[middle]) / 2.0
    }
}

/// The strip in [`Mode::Relative`]: the window's turns, each at the row it
/// starts on, plus the band.
///
/// `focus_row` is the reader's own row, in the same space as the viewport;
/// `None` means nothing is focused and the newest square is marked instead.
///
/// Two rules bound how many squares a strip holds, and both are in rows because
/// that is the space the strip is drawn in:
///
///   - **spacing**: a square is kept only if it clears `options.min_gap` of the
///     newer square below it, so two turns a row apart both draw while two that
///     share a row do not. The newest win, which is where the reader is.
///   - **budget**: `options.max_squares` caps the count outright (`0` leaves the
///     window as the only bound, and the window holds one square per row). The
///     reader's own turn is kept either way, even when it is the oldest.
pub fn relative_layout(
    placements: &[Placement],
    focus_row: Option<i64>,
    viewport: Viewport,
    pins: &[String],
    options: &Options,
) -> RelativeStrip {
    let top = viewport.top as f64;
    let bottom = viewport.bottom as f64;
    let mut candidates: Vec<(usize, f64)> = placements
        .iter()
        .enumerate()
        .filter(|(_, placement)| drawn_at_all(placement.kind, options))
        .filter_map(|(index, placement)| window_row(placement, top, bottom).map(|row| (index, row)))
        .collect();
    candidates.sort_by(|left, right| left.1.total_cmp(&right.1));
    let rows = bottom - top + 1.0;
    if candidates.is_empty() {
        let head = band(pins, &[], options);
        return RelativeStrip {
            band: head.len(),
            squares: head,
            rows,
        };
    }

    let focused = focus_row.map(|row| row as f64 - top);
    let focus_owner = focus_row.and_then(|row| {
        let row = row as f64;
        // Newest first: when two turns' spans meet on the reader's row, the
        // newer one owns it, which is the one the reader is heading into.
        placements.iter().rposition(|placement| {
            let (start, end) = extent(placement);
            drawn_at_all(placement.kind, options)
                && start.is_finite()
                && row >= start
                && row <= end
        })
    });

    // Newest first, one pass: a square survives when it clears the gap of the
    // newest square already kept.
    let mut kept: Vec<(usize, f64)> = Vec::new();
    for &(index, row) in candidates.iter().rev() {
        let clashes = kept
            .last()
            .is_some_and(|&(_, newest)| row > newest - options.min_gap);
        if !clashes {
            kept.push((index, row));
        }
    }

    // The reader's own turn is never the one the gap drops: it is the square the
    // strip exists to place. Whatever sat within a gap of it goes instead.
    if let Some(owner) = focus_owner {
        if let Some(&entry) = candidates.iter().find(|&&(index, _)| index == owner) {
            if !kept.iter().any(|&(index, _)| index == owner) {
                kept.retain(|&(_, row)| (row - entry.1).abs() >= options.min_gap);
                kept.push(entry);
            }
        }
    }

    if options.max_squares > 0 && kept.len() > options.max_squares {
        let mut ordered = kept.clone();
        ordered.sort_by(|left, right| right.1.total_cmp(&left.1));
        let mut trimmed: Vec<(usize, f64)> = Vec::with_capacity(options.max_squares);
        if let Some(&entry) = kept.iter().find(|&&(index, _)| Some(index) == focus_owner) {
            trimmed.push(entry);
        }
        for entry in ordered {
            if trimmed.len() >= options.max_squares {
                break;
            }
            if trimmed.iter().any(|&(index, _)| index == entry.0) {
                continue;
            }
            trimmed.push(entry);
        }
        kept = trimmed;
    }

    kept.sort_by(|left, right| left.1.total_cmp(&right.1));

    // Exactly one square is the one being read: the turn the reader's row is in,
    // or — when that turn is not in the window — the square nearest that row.
    let active = match kept.is_empty() {
        true => None,
        false => {
            let by_focus = kept
                .iter()
                .position(|&(index, _)| Some(index) == focus_owner)
                .or_else(|| {
                    focused.map(|row| {
                        let mut best = 0;
                        for (position, &(_, other)) in kept.iter().enumerate() {
                            if (other - row).abs() < (kept[best].1 - row).abs() {
                                best = position;
                            }
                        }
                        best
                    })
                });
            Some(by_focus.unwrap_or(kept.len() - 1))
        }
    };

    let reference = reference_of(placements, &kept);
    let mut squares: Vec<Square> = kept
        .iter()
        .enumerate()
        .map(|(position, &(index, row))| {
            let placement = &placements[index];
            Square {
                id: placement.id.clone(),
                kind: placement.kind,
                y: row,
                scale: clamp(
                    1.0 + options.ratio_flex * (placement.total as f64 / 1.0f64.max(reference) - 1.0),
                    options.scale_min,
                    options.scale_max,
                ),
                active: Some(position) == active,
            }
        })
        .collect();
    let drawn: Vec<String> = squares.iter().map(|square| square.id.clone()).collect();
    let mut head = band(pins, &drawn, options);
    let band = head.len();
    head.append(&mut squares);
    RelativeStrip {
        squares: head,
        band,
        rows,
    }
}

/// A buffer row to the map's own rows, cumulative over the placements and
/// continuous across the seams. Linear inside a turn, so anything else drawn in
/// the map's space agrees with the squares.
fn map_row(placements: &[Placement], row: f64) -> f64 {
    let mut behind = 0.0f64;
    for placement in placements {
        if row < placement.start {
            break;
        }
        if row <= placement.start + placement.rows - 1.0 {
            return behind + (row - placement.start);
        }
        behind += placement.rows;
    }
    behind
}

/// The strip in [`Mode::Map`]: every turn the window holds, at the row it falls
/// in it, plus the band.
///
/// `max_squares` trims the window to its newest turns *before* anything is
/// measured, because the cap changes `span` and with it every square's `y`: the
/// same pane at two caps is two different maps, and a caller must not mix their
/// output.
pub fn map_layout(
    placements: &[Placement],
    viewport: Viewport,
    pins: &[String],
    options: &Options,
) -> MapStrip {
    let shown: Vec<Placement> = placements
        .iter()
        .filter(|placement| drawn_at_all(placement.kind, options))
        .cloned()
        .collect();
    let capped: Vec<Placement> = if options.max_squares > 0 && shown.len() > options.max_squares {
        let mut ordered = shown;
        ordered.sort_by(|left, right| left.start.total_cmp(&right.start));
        ordered.split_off(ordered.len() - options.max_squares)
    } else {
        shown
    };

    let total: f64 = capped.iter().map(|placement| placement.rows).sum();
    let span = if total == 0.0 { 1.0 } else { total };

    let focus_row = viewport.top as f64;
    let active = match capped.is_empty() {
        true => None,
        false => Some(match capped.iter().position(|placement| {
            placement.start.is_finite()
                && focus_row >= placement.start
                && focus_row <= placement.start + placement.rows - 1.0
        }) {
            Some(index) => index,
            // A row past the last square, or above the first, falls back to the
            // end the reader is heading for.
            None if focus_row < capped[0].start => 0,
            None => capped.len() - 1,
        }),
    };

    let reference = {
        let mut totals: Vec<f64> = capped.iter().map(|placement| placement.total as f64).collect();
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

    let mut squares: Vec<Square> = capped
        .iter()
        .enumerate()
        .map(|(index, placement)| Square {
            id: placement.id.clone(),
            kind: placement.kind,
            y: map_row(&capped, placement.start),
            scale: clamp(
                1.0 + options.ratio_flex * (placement.total as f64 / 1.0f64.max(reference) - 1.0),
                options.scale_min,
                options.scale_max,
            ),
            active: Some(index) == active,
        })
        .collect();
    let drawn: Vec<String> = squares.iter().map(|square| square.id.clone()).collect();
    let mut head = band(pins, &drawn, options);
    let band = head.len();
    head.append(&mut squares);

    let top = map_row(&capped, viewport.top as f64);
    let block = Block {
        top,
        height: map_row(&capped, viewport.bottom as f64 + 1.0) - top,
    };
    MapStrip {
        squares: head,
        band,
        span,
        block,
    }
}

/// Rows and a viewport in, a strip out, in the mode `options` asks for.
///
/// The window is what the strip draws, so nothing is trimmed before it is
/// measured: `max_squares` is a budget on the window's own squares.
pub fn layout(rows: &[TurnRow], viewport: Viewport, focus_row: i64, options: &Options) -> Layout {
    layout_pinned(rows, &[], viewport, focus_row, options)
}

/// [`layout`], with the reader's own turns the window does not hold.
///
/// `pins` are the ids of turns the matcher found no rows for — the reader's
/// prompts above the window, oldest first — which is what the band is built
/// from. A caller that has no such list passes an empty slice and gets a strip
/// without a band.
pub fn layout_pinned(
    rows: &[TurnRow],
    pins: &[String],
    viewport: Viewport,
    focus_row: i64,
    options: &Options,
) -> Layout {
    let samples = samples_from(rows, viewport);
    let estimates = measure(&samples, viewport);
    let window = window_of(rows);
    let placements = place_window(&window, &samples, &estimates, viewport);
    match options.mode {
        Mode::Relative => Layout::Relative(relative_layout(
            &placements,
            Some(focus_row),
            viewport,
            pins,
            options,
        )),
        Mode::Map => Layout::Map(map_layout(&placements, viewport, pins, options)),
    }
}
