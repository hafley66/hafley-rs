//! What the viewport shows of a turn, and what that says about the turns
//! nobody can see.
//!
//! Two ports live here. `align_rows` is the terminal's row aligner — a source
//! line to the screen row it landed on — and it is what turns a turn's own text
//! into "how much of itself did the viewport hold". `samples_from`, `measure`
//! and `estimate_rows` are the estimator: measured wrap (`kappa`) and
//! unattributed gap (`gamma`), then one height per turn.

use boop_turnvis::{normalize_turn_line, LogicalLine, VisibleTurn};

use crate::_0_types::{
    clamp, kind_of, lines_of, Estimates, TurnRow, TurnSample, Viewport, WindowTurn, KINDS, ZEROED,
};

/// `String.prototype.length`: UTF-16 code units, not Unicode scalars. The
/// scoring below is a port of TypeScript that measures `.length`, so an
/// astral-plane character counts two here and one in `chars().count()`; the
/// containment bar and the tie-break score are both in code units.
fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// One source line against one screen line, the TypeScript score verbatim:
/// nothing for a line too short to be evidence, a dominant score for an exact
/// match, a weaker one for a containment that has enough text on both sides to
/// be more than a coincidence.
fn row_score(source_line: &str, screen_line: &str) -> i32 {
    let source_len = utf16_len(source_line);
    let screen_len = utf16_len(screen_line);
    if source_len < 3 || screen_len < 3 {
        return 0;
    }
    if source_line == screen_line {
        return 10_000 + source_len as i32;
    }
    if source_len >= 8
        && screen_len >= 8
        && (screen_line.contains(source_line) || source_line.contains(screen_line))
    {
        return 100 + source_len.min(screen_len) as i32;
    }
    0
}

/// `alignRegionRows`, over references so a caller can hand a filtered slice
/// without copying the rows' text.
///
/// Returns `(source index, matched row start)` in source order. The alignment
/// is the usual longest-common-subsequence dynamic program with the TypeScript
/// score as the weight: a cell takes the better of skipping a source line,
/// skipping a screen row, or pairing the two, and the backtrack walks from the
/// far corner preferring the pair, then up, then left.
fn align_refs(source: &[String], rows: &[&LogicalLine]) -> Vec<(usize, usize)> {
    let left: Vec<String> = source
        .iter()
        .map(|line| normalize_turn_line(line))
        .collect();
    let right: Vec<String> = rows
        .iter()
        .map(|row| normalize_turn_line(&row.text))
        .collect();
    let width = right.len() + 1;
    // The TypeScript allocates an Int32Array, zero-filled; the table is dense
    // and small (a pane's rows by a turn's lines).
    let mut values = vec![0i32; (left.len() + 1) * width];
    for source_row in 1..=left.len() {
        for screen_row in 1..=right.len() {
            let cell = source_row * width + screen_row;
            let matched = row_score(&left[source_row - 1], &right[screen_row - 1]);
            // Saturating where JavaScript's Int32Array would wrap: both are
            // unreachable for a real pane, and a panic in a debug build is not.
            values[cell] = values[(source_row - 1) * width + screen_row]
                .max(values[source_row * width + screen_row - 1])
                .max(if matched != 0 {
                    values[(source_row - 1) * width + screen_row - 1].saturating_add(matched)
                } else {
                    0
                });
        }
    }
    let mut matches: Vec<(usize, usize)> = Vec::new();
    let mut source_row = left.len();
    let mut screen_row = right.len();
    while source_row != 0 && screen_row != 0 {
        let cell = source_row * width + screen_row;
        let matched = row_score(&left[source_row - 1], &right[screen_row - 1]);
        if matched != 0
            && values[cell] == values[(source_row - 1) * width + screen_row - 1] + matched
        {
            matches.push((source_row - 1, rows[screen_row - 1].start));
            source_row -= 1;
            screen_row -= 1;
        } else if values[cell] == values[(source_row - 1) * width + screen_row] {
            source_row -= 1;
        } else {
            screen_row -= 1;
        }
    }
    matches.reverse();
    matches
}

/// Every source line that landed on a screen row, in source order. The
/// TypeScript took a `normalize` callback; here it is always
/// `boop_turnvis::normalize_turn_line`, which is the matcher's own normal form.
pub fn align_rows(source: &[String], rows: &[LogicalLine]) -> Vec<(usize, usize)> {
    let refs: Vec<&LogicalLine> = rows.iter().collect();
    align_refs(source, &refs)
}

/// One turn's row on the strip's ledger: how big it is, where it starts and
/// ends in the buffer, and how much of it the viewport holds.
///
/// `lines` counts the turn's own lines that the viewport's rows account for,
/// and it is a deliberate upgrade over the TypeScript path, which counted only
/// the rows a region projection had mapped: a turn the matcher never projected
/// still reports how much of itself showed. It is clamped to `1..=total` — a
/// turn on screen showed at least one line, and it cannot show more than it
/// has — so a caller can never read 0 out of it.
pub fn rows_of(screen: &[LogicalLine], turn: &VisibleTurn, viewport: Viewport) -> TurnRow {
    let said = if turn.role == "user" {
        boop_turnvis::boop_content(&turn.said)
    } else {
        &turn.said
    };
    let total = lines_of(said);
    let source: Vec<String> = said.split('\n').map(str::to_owned).collect();
    // The aligner only sees the rows the viewport holds, so every row it can
    // match is inside the viewport by construction.
    let visible: Vec<&LogicalLine> = screen
        .iter()
        .filter(|row| {
            let start = row.start as i64;
            start >= viewport.top && start <= viewport.bottom
        })
        .collect();
    let matched = align_refs(&source, &visible).len() as i64;
    TurnRow {
        id: turn.id.clone(),
        kind: kind_of(&turn.role),
        total,
        start: turn.buffer_start as i64,
        end: turn.buffer_end as i64,
        lines: matched.clamp(1, total),
    }
}

/// What the viewport shows of each turn that reaches it, oldest first.
///
/// The TypeScript read the turn's span and its per-line row mapping; here the
/// row already carries both (`rows_of` measured it), so this clips the span to
/// the viewport and drops the turns it misses entirely.
pub fn samples_from(rows: &[TurnRow], viewport: Viewport) -> Vec<TurnSample> {
    let mut samples: Vec<TurnSample> = Vec::new();
    for row in rows {
        let start = row.start.max(viewport.top);
        let end = row.end.min(viewport.bottom);
        if end < start {
            continue;
        }
        samples.push(TurnSample {
            id: row.id.clone(),
            kind: row.kind,
            turn_start: row.start,
            start,
            end,
            // A row with no line count at all (a turn nothing was measured of)
            // falls back to one line per visible row, which is the least it can
            // have shown.
            lines: if row.lines > 0 {
                row.lines
            } else {
                (end - start + 1).min(row.total)
            },
            total: row.total,
        });
    }
    samples.sort_by(|left, right| left.start.cmp(&right.start));
    samples
}

/// `kappa` and `gamma` from what is on screen.
///
/// `kappa` is measured wrap: the rows the viewport holds of a turn over the
/// lines it holds of the same turn, pooled per kind. Nothing is assumed about
/// wrapping — a pane that wraps every long line simply reports a bigger
/// `kappa`. `gamma` is the mean unattributed run between two adjacent sampled
/// turns (blank lines, separators, anything the matcher did not attribute),
/// which is what turns a line count into a distance down the screen.
pub fn measure(samples: &[TurnSample], viewport: Viewport) -> Estimates {
    let mut numerator = ZEROED;
    let mut denominator = ZEROED;
    let mut kappa_max = 1.0f64;
    for sample in samples {
        let rows = (sample.end - sample.start + 1) as f64;
        let lines = 1.0f64.max(sample.lines as f64);
        numerator[sample.kind.index()] += rows;
        denominator[sample.kind.index()] += lines;
        kappa_max = kappa_max.max(rows / lines);
    }
    let every_numerator: f64 = KINDS.iter().map(|kind| numerator[kind.index()]).sum();
    let every_denominator: f64 = KINDS.iter().map(|kind| denominator[kind.index()]).sum();
    let pooled = if every_denominator > 0.0 {
        clamp(every_numerator / every_denominator, 1.0, kappa_max)
    } else {
        1.0
    };
    let mut kappa = ZEROED;
    for kind in KINDS {
        let index = kind.index();
        kappa[index] = if denominator[index] > 0.0 {
            clamp(numerator[index] / denominator[index], 1.0, kappa_max)
        } else {
            // Nothing of this kind was on screen: the pooled wrap is the best
            // the window can say about it.
            pooled
        };
    }

    let mut gaps = ZEROED;
    let mut boundaries = ZEROED;
    for index in 1..samples.len() {
        let previous = &samples[index - 1];
        let current = &samples[index];
        let top = (previous.end + 1).max(viewport.top);
        let bottom = (current.start - 1).min(viewport.bottom);
        if bottom < top {
            continue;
        }
        gaps[current.kind.index()] += (bottom - top + 1) as f64;
        boundaries[current.kind.index()] += 1.0;
    }
    let every_gap: f64 = KINDS.iter().map(|kind| gaps[kind.index()]).sum();
    let every_boundary: f64 = KINDS.iter().map(|kind| boundaries[kind.index()]).sum();
    let pooled_gap = if every_boundary > 0.0 {
        every_gap / every_boundary
    } else {
        0.0
    };
    let mut gamma = ZEROED;
    for kind in KINDS {
        let index = kind.index();
        gamma[index] = if boundaries[index] > 0.0 {
            gaps[index] / boundaries[index]
        } else {
            pooled_gap
        };
    }
    Estimates {
        kappa,
        gamma,
        kappa_max,
    }
}

/// One turn's whole height when nothing of it is on screen. Clamped at both
/// ends: never fewer rows than the turn has lines, never more than the worst
/// wrap this window measured allows.
pub fn estimate_rows(turn: &WindowTurn, estimates: &Estimates) -> f64 {
    let total = turn.total as f64;
    clamp(
        estimates.kappa[turn.kind.index()] * total,
        total,
        estimates.kappa_max * total,
    )
}
