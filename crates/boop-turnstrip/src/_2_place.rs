//! Where every turn in the window sits in buffer rows, including the ones
//! nobody measured.
//!
//! A measured turn keeps its own span. The turns between two measured turns
//! share the exact gap the two of them left, in proportion to their estimated
//! size, so the interval's ends stay true. The turns outside the measured range
//! are stacked outward from the nearest anchor. A last forward pass keeps the
//! order from inverting.

use std::collections::HashMap;

use crate::_0_types::{clamp, Estimates, Placement, TurnRow, TurnSample, Viewport, WindowTurn};
use crate::_1_measure::estimate_rows;

/// The window's rows, oldest first, as the estimator wants them. The row
/// arrives in transcript order, but the estimator sorts by buffer row, so the
/// sort key is paired with each turn before the key is dropped.
pub fn window_of(rows: &[TurnRow]) -> Vec<WindowTurn> {
    let mut keyed: Vec<(i64, WindowTurn)> = rows
        .iter()
        .map(|row| {
            (
                row.start,
                WindowTurn {
                    id: row.id.clone(),
                    kind: row.kind,
                    total: row.total,
                },
            )
        })
        .collect();
    keyed.sort_by(|left, right| left.0.cmp(&right.0));
    keyed.into_iter().map(|(_, turn)| turn).collect()
}

/// Every turn in the window, placed in buffer rows. See the module docs for
/// the three passes, which run in this order: outward stacking from the anchor,
/// the proportional gap fill for a run between two measured turns, then the
/// order-only repair that adds no overhead.
pub fn place_window(
    window: &[WindowTurn],
    samples: &[TurnSample],
    estimates: &Estimates,
    viewport: Viewport,
) -> Vec<Placement> {
    let by_id: HashMap<&str, &TurnSample> = samples
        .iter()
        .map(|sample| (sample.id.as_str(), sample))
        .collect();
    let mut placements: Vec<Placement> = window
        .iter()
        .map(|turn| match by_id.get(turn.id.as_str()) {
            None => Placement {
                id: turn.id.clone(),
                kind: turn.kind,
                // Unplaced until a pass below writes it; the same NaN sentinel
                // the TypeScript used, so a caller that ignores the placement
                // passes sees the same thing.
                start: f64::NAN,
                rows: estimate_rows(turn, estimates),
                total: turn.total,
                seen: 1.0,
                measured: false,
                visible: None,
            },
            Some(sample) => {
                let rows = (sample.end - sample.start + 1) as f64;
                let seen = clamp(
                    sample.lines as f64 / 1.0f64.max(sample.total as f64),
                    1.0 / 1.0f64.max(rows),
                    1.0,
                );
                let hidden = 0.0f64.max((sample.total - sample.lines) as f64);
                Placement {
                    id: turn.id.clone(),
                    kind: turn.kind,
                    // The turn's own first row is truth; only its height is
                    // inferred, from the lines still off screen at this kind's
                    // measured wrap.
                    start: sample.turn_start as f64,
                    rows: rows + hidden * estimates.kappa[sample.kind.index()],
                    total: sample.total,
                    seen,
                    measured: true,
                    visible: Some((sample.start as f64, sample.end as f64)),
                }
            }
        })
        .collect();
    if placements.is_empty() {
        return placements;
    }

    let anchor = placements
        .iter()
        .position(|placement| placement.measured)
        .map(|index| index as i64)
        .unwrap_or(-1);
    if anchor < 0 {
        placements[0].start = viewport.top as f64;
    }
    // Outward, backwards: everything above the anchor stacks up from it, and
    // pays the gap the newer neighbour leaves.
    let mut index = anchor - 1;
    while index >= 0 {
        let next = (index + 1) as usize;
        let here = index as usize;
        let above = placements[next].start - placements[here].rows;
        placements[here].start = above - estimates.gamma[placements[next].kind.index()];
        index -= 1;
    }
    let first = if anchor < 0 { 0 } else { anchor };
    let mut index = first + 1;
    while index < placements.len() as i64 {
        let here = index as usize;
        if !placements[here].measured {
            let previous = here - 1;
            let below = placements[previous].start + placements[previous].rows;
            placements[here].start = below + estimates.gamma[placements[here].kind.index()];
        }
        index += 1;
    }

    // A run of unmeasured turns between two measured ones: each owns a share of
    // the exact gap its neighbours left, sized by its estimate plus its own
    // overhead, and sits centred in that share.
    let mut run_start: i64 = -1;
    let mut index: i64 = 0;
    while index <= placements.len() as i64 {
        let open = index < placements.len() as i64 && !placements[index as usize].measured;
        if open {
            if run_start < 0 {
                run_start = index;
            }
            index += 1;
            continue;
        }
        let before = run_start - 1;
        if run_start >= 0
            && before >= 0
            && index < placements.len() as i64
            && placements[index as usize].measured
        {
            let run_len = (index - run_start) as usize;
            let weights: Vec<f64> = (0..run_len)
                .map(|offset| {
                    let placement = &placements[run_start as usize + offset];
                    1.0f64.max(placement.rows + estimates.gamma[placement.kind.index()])
                })
                .collect();
            let weight: f64 = weights.iter().sum();
            let gap = 0.0f64.max(
                placements[index as usize].start
                    - (placements[before as usize].start + placements[before as usize].rows),
            );
            let mut cursor = placements[before as usize].start + placements[before as usize].rows;
            for offset in 0..run_len {
                let here = run_start as usize + offset;
                let share = (weights[offset] / weight) * gap;
                placements[here].start = cursor + (share - placements[here].rows) / 2.0;
                cursor += share;
            }
        }
        run_start = -1;
        index += 1;
    }

    // Order only: every earlier pass accounted for its own overhead, so this
    // repair may not add any of it back. NaN never reaches here — every
    // unmeasured start was written by the passes above — so `max` cannot pick
    // the wrong side of a NaN.
    for index in 1..placements.len() {
        if placements[index].measured {
            continue;
        }
        let floor = placements[index - 1].start + placements[index - 1].rows;
        placements[index].start = placements[index].start.max(floor);
    }
    placements
}
