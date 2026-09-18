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
//!     of the squares, whatever the gap or the cap says. A square is also as
//!     big as the part of its turn the reader can see: the placement's own
//!     intersection with the window, floored at `options.scale_min`, so a turn
//!     scrolled halfway out is half a square, a turn wholly in view is full
//!     size, and a sliver keeps the floor. The scroll is the animation —
//!     re-projecting after a scroll changes a square's size and its row, never
//!     the turn it names.
//!   - [`Mode::Recent`]: the strip is the newest turns of the session, one
//!     square each, uniform, oldest first, read from the store rather than from
//!     the window — a turn the window lost is a member like any other. `y`
//!     counts places in the block, no row and no span, so the caller centres the
//!     block on its own track and a scroll moves nothing at all.
//!
//! Relative mode keeps the reader's own turns on the strip even when it places
//! none of them: the *band*, a short run of pinned squares at the head of the
//! layout. Those are the turns a reader navigates by — the prompts they wrote —
//! so losing them to a scroll is worse than drawing them off-position. They are
//! not positions and carry no row: `y` counts places in the band. A recent strip
//! needs no band, because the list of the newest turns already holds the
//! reader's own prompts.
//!
//! Only the conversation draws in either mode: [`drawn_at_all`] is the one
//! place that decides, and tool calls, tool results and the turns the CLI wrote
//! on the user's behalf are not part of it. Relative mode draws one more thing
//! beside the conversation: the tools, as tiny squares, through their own
//! predicate [`drawn_as_tool`]. A recent strip does not, because the recency
//! list is the conversation's and the gap marker already covers tool focus.
//!
//! `layout` is the whole pipeline in one call — rows to samples, samples to
//! `kappa`/`gamma`, placements, strip — and it is the entry point a server,
//! a CLI or a wasm binding uses.

use crate::_0_types::{
    clamp, Layout, ListedTurn, Mode, Options, Placement, RecentStrip, RelativeStrip, Square,
    TurnKind, TurnRow, Viewport, DEFAULT_RECENT_MAX,
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
///
/// They draw uniform, at `1.0`: a pin is an id and nothing else, so the turn has
/// no rows here and the band has no intersection with the window to size by.
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

/// Whether a turn draws at all. The conversation is the reader's own prompts and
/// the agent's answers; a tool call, a tool result and everything the CLI wrote
/// on the user's behalf (`meta`) are not, and never draw in either mode.
///
/// The filter lives in one place so a hidden turn costs no square anywhere: not
/// a place in the band, not a slot in the spacing pass, not a step of the recent
/// block. A caller building a recency list from its own store filters with this
/// same predicate, so the policy is written down once.
pub fn drawn_at_all(kind: TurnKind) -> bool {
    matches!(kind, TurnKind::User | TurnKind::Agent)
}

/// Whether a turn draws as a tiny tool square in relative mode, beside but not
/// through [`drawn_at_all`]. The conversation predicate stays what it is: it
/// is the one gate for the band, the recency list and the budget, and a tool
/// never enters any of those. A tool is still something the reader wants to see
/// fly past on the strip, so the relative pass admits it by this predicate next
/// to the conversation's. The two differ on purpose: [`drawn_at_all`] answers
/// "is this the conversation", which owns the spacing and the budget, while
/// this one answers "is this a tool to mark", which owns only a tiny square.
/// Recent mode never asks it, because the recency list is the conversation's
/// and the gap marker already covers tool focus.
pub fn drawn_as_tool(kind: TurnKind) -> bool {
    matches!(kind, TurnKind::Tool)
}

/// A tool square's fixed size. Tools are markers, not turns: nothing sizes one
/// from its own intersection with the window, because a tool call is not a turn
/// the reader reads, and one square at this scale is what a dense run of tools
/// needs to not crowd the conversation.
const TOOL_SCALE: f64 = 0.35;

/// The most tool squares one layout draws, newest first, the oldest dropped
/// silently: the same drop-from-front cutoff as [`DEFAULT_RECENT_MAX`]. A run
/// of tools in one turn can outlast the window, and tools may never crowd the
/// conversation, so a hard budget keeps the newest on the strip.
const TOOL_BUDGET: usize = 16;

/// Least distance between two kept tool squares, in window rows. Fixed, not
/// `options.min_gap`: the conversation's own gap is the reader's spacing, and a
/// dense run of tools must compress against this smaller bar instead of washing
/// the track.
const TOOL_GAP: f64 = 2.0;

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
///
/// A square's size is the reader's own intersection with its turn: the
/// placement's `seen` — the fraction of the turn inside the viewport — floored
/// at `options.scale_min` and capped at the plain square. A turn wholly in view
/// draws full size, a turn scrolled halfway out draws half a square, and a
/// sliver keeps the floor so it stays findable. Nothing here sizes a square
/// against its neighbours: how long a turn is says nothing about how much of it
/// the reader can see, which is the one thing the size is about.
///
/// Beside the conversation, the same window's tools draw as tiny squares, each
/// on its own first visible row. Tools are not conversation: they never enter
/// the spacing or budget passes above (a conversation square keeps its
/// `min_gap` eviction untouched, and a tool never evicts anything), they hold
/// their own [`TOOL_BUDGET`] with their own [`TOOL_GAP`] spacing, and they draw
/// at the fixed [`TOOL_SCALE`], never `active`, never in the band.
pub fn relative_layout(
    placements: &[Placement],
    _focus_row: Option<i64>,
    viewport: Viewport,
    pins: &[String],
    options: &Options,
) -> RelativeStrip {
    let top = viewport.top as f64;
    let bottom = viewport.bottom as f64;
    let mut candidates: Vec<(usize, f64)> = placements
        .iter()
        .enumerate()
        .filter(|(_, placement)| drawn_at_all(placement.kind))
        .filter_map(|(index, placement)| window_row(placement, top, bottom).map(|row| (index, row)))
        .collect();
    candidates.sort_by(|left, right| left.1.total_cmp(&right.1));
    let rows = bottom - top + 1.0;

    // Physical intersections remain represented even when the spacing or
    // recent-history budget would otherwise discard them.
    let mut window: Vec<Square> = candidates
        .iter()
        .map(|&(index, row)| {
            let placement = &placements[index];
            Square {
                id: placement.id.clone(),
                kind: placement.kind,
                y: row,
                scale: clamp(placement.seen, options.scale_min, 1.0),
                active: true,
            }
        })
        .collect();
    // Tools draw beside the conversation, on their own rows, through their own
    // predicate, spacing and budget. A tool is never `active` and never enters
    // the band, so `drawn` below is only ever the conversation's own ids.
    window.extend(tool_squares(placements, viewport));
    // Both the conversation's and the tools' squares are in window rows, so
    // they interleave in the one row-sorted list.
    window.sort_by(|left, right| left.y.total_cmp(&right.y));
    let drawn: Vec<String> = window
        .iter()
        .filter(|square| drawn_at_all(square.kind))
        .map(|square| square.id.clone())
        .collect();
    let mut head = band(pins, &drawn, options);
    let band = head.len();
    head.append(&mut window);
    RelativeStrip {
        squares: head,
        band,
        rows,
        gap: None,
    }
}

/// The tool squares a relative strip draws: the window's visible tools, newest
/// first, each only if it clears [`TOOL_GAP`] of the newest tool already kept
/// and at most [`TOOL_BUDGET`] of them in all, the oldest dropped silently.
///
/// Tools are admitted by [`drawn_as_tool`], never by [`drawn_at_all`], so none
/// of them enters the conversation's spacing or budget passes: a conversation
/// square keeps its `min_gap` eviction untouched, and a tool never evicts
/// anything: it only ever decides not to draw itself. A dense run of tools
/// compresses against the fixed [`TOOL_GAP`] (not `options.min_gap`, which is
/// the conversation's own spacing) so it does not wash the track, and it stops
/// at [`TOOL_BUDGET`], the newest surviving, because many tools fly in one turn
/// and the strip is the conversation's first.
///
/// Every tool draws at [`TOOL_SCALE`], fixed: a tool call is a marker, not a
/// turn the reader reads, so nothing sizes it from its intersection with the
/// window. `placements` is oldest first, so the newest is the highest index;
/// walking from the back puts the newest first, which is the end a dense run
/// keeps.
fn tool_squares(placements: &[Placement], viewport: Viewport) -> Vec<Square> {
    let top = viewport.top as f64;
    let bottom = viewport.bottom as f64;
    let mut kept: Vec<(usize, f64)> = placements
        .iter()
        .enumerate()
        .filter(|(_, placement)| drawn_as_tool(placement.kind))
        .filter_map(|(index, placement)| window_row(placement, top, bottom).map(|row| (index, row)))
        .collect();
    // Newest first: the highest placement index, which is where the reader is.
    kept.sort_by(|left, right| right.0.cmp(&left.0));
    let mut squares: Vec<Square> = Vec::new();
    for (index, row) in kept {
        if let Some(newest) = squares.first() {
            // A tool draws only if it clears the newest tool already kept, so a
            // dense run compresses instead of washing the track.
            if (newest.y - row).abs() < TOOL_GAP {
                continue;
            }
        }
        if squares.len() >= TOOL_BUDGET {
            continue;
        }
        let placement = &placements[index];
        squares.push(Square {
            id: placement.id.clone(),
            kind: TurnKind::Tool,
            y: row,
            scale: TOOL_SCALE,
            active: false,
        });
    }
    // Back to window-row order, so the interleave in `relative_layout` is
    // correct after all tools share the one sort.
    squares.sort_by(|left, right| left.y.total_cmp(&right.y));
    squares
}

/// The strip in [`Mode::Recent`]: the newest turns of the session, one square
/// each, oldest first, uniform.
///
/// `listed` is the session's turns as the caller reads them from the store,
/// oldest first, ids and kinds only: the window's rows are no help here, because
/// a turn the window lost is a member like any other. Only the turns
/// [`drawn_at_all`] admits are listed, so a chatty tool takes no place.
///
/// `focus` is the id of the turn the reader's top row is inside, when there is
/// one. If it names no square — a tool turn, or a row past the end — the newest
/// square is active instead: the end the reader is heading for.
///
/// `max_squares` caps the count by dropping from the FRONT, so the oldest places
/// fall off silently, with no marker to say how many did. A caller that leaves
/// it at `0` gets [`DEFAULT_RECENT_MAX`]: recent mode has no window to bound it,
/// and a block taller than the pane is a list whose end the reader cannot reach.
///
/// Every square draws at `1.0`. Relative mode sizes a square by the reader's
/// intersection with its turn, and a place in a list has neither rows nor a
/// viewport to intersect: the list is a list of places, so there is no
/// intersection to scale by and every place is the same size.
pub fn recent_layout(
    listed: &[ListedTurn],
    focus: Option<&str>,
    viewport: Viewport,
    options: &Options,
) -> RecentStrip {
    let mut turns: Vec<&ListedTurn> = listed
        .iter()
        .filter(|turn| drawn_at_all(turn.kind))
        .collect();
    let cap = match options.max_squares {
        0 => DEFAULT_RECENT_MAX,
        set => set,
    };
    if turns.len() > cap {
        turns.drain(..turns.len() - cap);
    }
    let active = match turns.is_empty() {
        true => None,
        false => Some(
            turns
                .iter()
                .position(|turn| Some(turn.id.as_str()) == focus)
                .unwrap_or(turns.len() - 1),
        ),
    };
    let squares: Vec<Square> = turns
        .iter()
        .enumerate()
        .map(|(place, turn)| Square {
            id: turn.id.clone(),
            kind: turn.kind,
            y: place as f64,
            scale: 1.0,
            active: Some(place) == active,
        })
        .collect();
    RecentStrip {
        squares,
        gap: None,
        // The same track measurement relative mode reports, so a caller centres
        // both blocks in one space.
        rows: (viewport.bottom as f64 - viewport.top as f64 + 1.0).max(1.0),
    }
}

/// Rows and a viewport in, a strip out, in the mode `options` asks for.
///
/// [`Mode::Relative`] reads `rows`, and nothing is trimmed before it is
/// measured: `max_squares` is a budget on the window's own squares.
/// [`Mode::Recent`] reads the session's turns instead, which this call has none
/// of — a recency list cannot be built from the window, so it draws an empty
/// strip. Call [`layout_pinned`] to hand it a list.
pub fn layout(rows: &[TurnRow], viewport: Viewport, focus_row: i64, options: &Options) -> Layout {
    layout_pinned(rows, &[], &[], viewport, focus_row, options)
}

/// [`layout`], with the reader's own turns the window does not hold and the
/// session's turns a recency list draws.
///
/// `pins` are the ids of turns the matcher found no rows for — the reader's
/// prompts above the window, oldest first — which is what the band is built
/// from. A caller that has no such list passes an empty slice and gets a strip
/// without a band. Only [`Mode::Relative`] reads them.
///
/// `listed` is the session's turns, oldest first, ids and kinds only, which is
/// what [`recent_layout`] draws. Only [`Mode::Recent`] reads it.
pub fn layout_pinned(
    rows: &[TurnRow],
    pins: &[String],
    listed: &[ListedTurn],
    viewport: Viewport,
    focus_row: i64,
    options: &Options,
) -> Layout {
    let samples = samples_from(rows, viewport);
    let estimates = measure(&samples, viewport);
    let window = window_of(rows);
    let placements = place_window(&window, &samples, &estimates, viewport);
    let gap = crate::_3a_gap::visible_gap(rows, viewport);
    let mut result = match options.mode {
        Mode::Relative => Layout::Relative(relative_layout(
            &placements,
            Some(focus_row),
            viewport,
            pins,
            options,
        )),
        Mode::Recent => {
            // Newest first: when two turns' spans meet on the reader's row, the
            // newer one owns it, which is the one the reader is heading into.
            let focus = rows
                .iter()
                .rposition(|row| row.start <= focus_row && focus_row <= row.end)
                .map(|index| rows[index].id.as_str());
            let recent = recent_layout(listed, focus, viewport, options);
            let retained: Vec<_> = listed
                .iter()
                .filter(|turn| {
                    recent.squares.iter().any(|square| square.id == turn.id)
                        || rows.iter().any(|row| {
                            row.id == turn.id
                                && row.start <= viewport.bottom
                                && row.end >= viewport.top
                        })
                })
                .cloned()
                .collect();
            let expanded = Options {
                max_squares: retained.len(),
                ..options.clone()
            };
            Layout::Recent(recent_layout(&retained, focus, viewport, &expanded))
        }
    };
    match &mut result {
        Layout::Relative(strip) => strip.gap = gap,
        Layout::Recent(strip) => {
            for square in &mut strip.squares {
                square.active = rows.iter().any(|row| {
                    row.id == square.id && row.start <= viewport.bottom && row.end >= viewport.top
                });
            }
            strip.gap = gap;
        }
    }
    result
}
