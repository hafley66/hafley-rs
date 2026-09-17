//! The TypeScript fixture, ported: one claude pane mid-stream, a reader who has
//! scrolled so that two of its five turns are on screen, and the rest of the
//! rolling window extrapolated from those two.
//!
//! Every expected number below is the inline snapshot in
//! `1_agentSquaresEstimate.test.ts`, which pins them to two decimals the way
//! `Number(value.toFixed(2))` pins them there; `close` is the same rounding
//! stated as a tolerance. The rows and lines the matcher reported are replayed
//! as a grid — each attributed row carries the turn line that landed on it —
//! and the empty rows stay empty, which is what the aligner must ignore.

use boop_turnstrip::{
    layout, layout_pinned, map_layout, measure, place_window, relative_layout, rows_of, samples_from,
    window_of, Layout, Mode, Options, Placement, RelativeStrip, TurnKind, TurnRow, Viewport, KINDS,
};
use boop_turnvis::{Confidence, LogicalLine, VisibleTurn};

/// One projection turn: the span the matcher found, its line count, and the
/// buffer rows its lines landed on.
struct TurnSpec {
    id: String,
    role: &'static str,
    span: (i64, i64),
    lines: i64,
    rows: Vec<i64>,
}

fn spec(id: &str, role: &'static str, span: (i64, i64), lines: i64, rows: &[i64]) -> TurnSpec {
    TurnSpec {
        id: id.to_string(),
        role,
        span,
        lines,
        rows: rows.to_vec(),
    }
}

fn said(lines: i64) -> String {
    (1..=lines)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A claude pane mid-stream: a one-line prompt, a long answer that wraps, a SQL
/// result, a short answer, a short tool result. Rows 2, 31-32, 51 and 59-60
/// hold nothing the matcher attributed — blank lines and the result's own
/// separator.
fn pane() -> Vec<TurnSpec> {
    vec![
        spec("u1", "user", (0, 1), 1, &[0]),
        spec(
            "a2",
            "assistant",
            (3, 30),
            24,
            &[
                3, 5, 7, 9, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28,
                29, 30,
            ],
        ),
        spec(
            "t3",
            "tool",
            (33, 50),
            12,
            &[33, 35, 37, 39, 41, 43, 45, 46, 47, 48, 49, 50],
        ),
        spec("a4", "assistant", (52, 58), 6, &[52, 54, 55, 56, 57, 58]),
        spec("t5", "tool", (61, 63), 2, &[61, 63]),
    ]
}

/// The reader has scrolled so the SQL result's tail and the short answer are on
/// screen: 24 rows, of which t3 shows 10 of its 12 lines and a4 all 6 of its.
const VIEWPORT: Viewport = Viewport {
    top: 37,
    bottom: 60,
};

/// The same pane with the turn the matcher dropped (`x2`) in it, and the two
/// neighbours whose gap has to hold it.
fn gap() -> Vec<TurnSpec> {
    vec![
        spec("x1", "assistant", (0, 9), 5, &[0, 2, 4, 6, 8]),
        spec("x3", "assistant", (23, 32), 4, &[23, 25, 27, 29]),
    ]
}

fn all() -> Vec<TurnSpec> {
    vec![
        spec("x1", "assistant", (0, 9), 5, &[0, 2, 4, 6, 8]),
        spec("x2", "tool", (12, 20), 3, &[12, 15, 18]),
        spec("x3", "assistant", (23, 32), 4, &[23, 25, 27, 29]),
    ]
}

/// `count` turns, two rows apart, one line each: the row cap's fixture.
fn many(count: i64) -> Vec<TurnSpec> {
    (0..count)
        .map(|index| {
            spec(
                &format!("s{}", index + 1),
                "assistant",
                (index * 2, index * 2 + 1),
                1,
                &[index * 2],
            )
        })
        .collect()
}

fn turn_of(spec: &TurnSpec) -> VisibleTurn {
    VisibleTurn {
        session: "s1".to_string(),
        harness: "claude".to_string(),
        turn: 0,
        ts: 0,
        role: spec.role.to_string(),
        said: said(spec.lines),
        id: spec.id.clone(),
        buffer_start: spec.span.0 as usize,
        buffer_end: spec.span.1 as usize,
        anchor_start: spec.span.0 as usize,
        anchor_end: spec.span.1 as usize,
        confidence: Confidence::Anchored,
    }
}

/// The pane's grid: every attributed row carries the turn line that landed on
/// it, and every other row is empty — the blank lines and the separator the
/// matcher attributed nothing to.
fn grid(specs: &[TurnSpec]) -> Vec<LogicalLine> {
    let last = specs
        .iter()
        .flat_map(|spec| spec.rows.iter().copied())
        .max()
        .unwrap_or(-1);
    let mut lines: Vec<LogicalLine> = (0..=last)
        .map(|row| LogicalLine {
            text: String::new(),
            start: row as usize,
            end: row as usize,
        })
        .collect();
    for spec in specs {
        assert_eq!(
            spec.rows.len() as i64,
            spec.lines,
            "{} has a row per line",
            spec.id
        );
        for (offset, row) in spec.rows.iter().enumerate() {
            lines[*row as usize].text = format!("line {}", offset + 1);
        }
    }
    lines
}

fn rows_for(specs: &[TurnSpec], viewport: Viewport) -> Vec<TurnRow> {
    let grid = grid(specs);
    specs
        .iter()
        .map(|spec| rows_of(&grid, &turn_of(spec), viewport))
        .collect()
}

fn placements_of(rows: &[TurnRow], viewport: Viewport) -> Vec<Placement> {
    let samples = samples_from(rows, viewport);
    place_window(
        &window_of(rows),
        &samples,
        &measure(&samples, viewport),
        viewport,
    )
}

/// The TypeScript pins its snapshots to two decimals, so a match is a value
/// that rounds to the same hundredth.
fn close(got: f64, want: f64) -> bool {
    (got - want).abs() <= 0.005 + 1e-9
}

#[test]
fn reads_the_viewport_the_way_the_matcher_reported_it() {
    let samples = samples_from(&rows_for(&pane(), VIEWPORT), VIEWPORT);
    assert_eq!(samples.len(), 2, "only the reached turns are sampled");
    let t3 = &samples[0];
    assert_eq!(
        (
            t3.id.as_str(),
            t3.kind,
            t3.turn_start,
            t3.start,
            t3.end,
            t3.lines,
            t3.total
        ),
        ("t3", TurnKind::Tool, 33, 37, 50, 10, 12)
    );
    let a4 = &samples[1];
    assert_eq!(
        (
            a4.id.as_str(),
            a4.kind,
            a4.turn_start,
            a4.start,
            a4.end,
            a4.lines,
            a4.total
        ),
        ("a4", TurnKind::Agent, 52, 52, 58, 6, 6)
    );
}

#[test]
fn measures_wrap_and_gap_overhead_from_what_is_on_screen() {
    let samples = samples_from(&rows_for(&pane(), VIEWPORT), VIEWPORT);
    let estimates = measure(&samples, VIEWPORT);
    let kappa = |kind: TurnKind| estimates.kappa[kind.index()];
    assert!(close(kappa(TurnKind::Agent), 1.1666666666666667));
    assert!(close(kappa(TurnKind::Other), 1.3125));
    assert!(close(kappa(TurnKind::Tool), 1.4));
    assert!(close(kappa(TurnKind::User), 1.3125));
    assert!(close(estimates.kappa_max, 1.4));
    for kind in KINDS {
        assert!(
            close(estimates.gamma[kind.index()], 1.0),
            "{kind:?} overhead could not be measured, so every other kind's is pooled"
        );
    }
}

#[test]
fn places_every_turn_in_the_window_when_only_two_of_them_are_visible() {
    let placements = placements_of(&rows_for(&pane(), VIEWPORT), VIEWPORT);
    let expected: [(&str, TurnKind, bool, f64, f64, f64, i64); 5] = [
        ("u1", TurnKind::User, false, 1.3125, 1.0, 1.6875, 1),
        ("a2", TurnKind::Agent, false, 28.0, 1.0, 4.0, 24),
        (
            "t3",
            TurnKind::Tool,
            true,
            16.8,
            0.8333333333333334,
            33.0,
            12,
        ),
        ("a4", TurnKind::Agent, true, 7.0, 1.0, 52.0, 6),
        ("t5", TurnKind::Tool, false, 2.8, 1.0, 60.0, 2),
    ];
    assert_eq!(placements.len(), expected.len());
    for (placement, (id, kind, measured, rows, seen, start, total)) in
        placements.iter().zip(expected)
    {
        assert_eq!(placement.id.as_str(), id);
        assert_eq!(placement.kind, kind);
        assert_eq!(placement.measured, measured);
        assert_eq!(placement.total, total);
        assert!(close(placement.rows, rows), "{id} rows {}", placement.rows);
        assert!(close(placement.seen, seen), "{id} seen {}", placement.seen);
        assert!(
            close(placement.start, start),
            "{id} start {}",
            placement.start
        );
    }
}

#[test]
fn keeps_the_measured_turns_at_their_own_rows_and_stacks_the_rest_around_them() {
    let specs = pane();
    let placements = placements_of(&rows_for(&specs, VIEWPORT), VIEWPORT);
    for placement in placements.iter().filter(|placement| placement.measured) {
        let turn = specs
            .iter()
            .find(|spec| spec.id == placement.id)
            .expect("a measured placement is a turn of the pane");
        assert!(
            close(placement.start, turn.span.0 as f64),
            "{} sits at {} and not at its own first row {}",
            placement.id,
            placement.start,
            turn.span.0
        );
    }
    // The next turn sits one unattributed row below the one before it, which is
    // the gap the window measured.
    assert!(close(
        placements[4].start,
        placements[3].start + placements[3].rows + 1.0
    ));
}

#[test]
fn lays_the_strip_out_in_the_windows_own_rows() {
    let placements = placements_of(&rows_for(&pane(), VIEWPORT), VIEWPORT);
    let strip = relative_layout(&placements, Some(40), VIEWPORT, &[], &Options::default());
    // The window is rows 37..=60, so the two turns above it are not in the
    // strip at all, `t3` (which starts at 33) draws on the window's first row
    // because that is where the reader met it, and `a4` draws on its own. `t5`
    // is *estimated* to start at 60, inside the window, and is not drawn: only a
    // turn the matcher saw on these rows has a row to name.
    let expected: [(&str, TurnKind, f64, f64, bool); 2] = [
        ("t3", TurnKind::Tool, 0.0, 1.1166666666666667, true),
        ("a4", TurnKind::Agent, 15.0, 0.8833333333333333, false),
    ];
    assert_eq!(strip.squares.len(), expected.len());
    for (square, (id, kind, y, scale, active)) in strip.squares.iter().zip(expected) {
        assert_eq!(square.id.as_str(), id);
        assert_eq!(square.kind, kind);
        assert_eq!(square.active, active);
        assert!(close(square.y, y), "{id} y {}", square.y);
        assert!(close(square.scale, scale), "{id} scale {}", square.scale);
    }
}

#[test]
fn marks_the_square_the_reader_is_looking_at_and_the_nearest_when_nothing_is() {
    let placements = placements_of(&rows_for(&pane(), VIEWPORT), VIEWPORT);
    let active = |row: Option<i64>| {
        relative_layout(&placements, row, VIEWPORT, &[], &Options::default())
            .squares
            .iter()
            .position(|square| square.active)
    };
    assert_eq!(
        [
            active(Some(40)),
            active(Some(56)),
            active(Some(0)),
            active(Some(999)),
            active(None),
        ],
        [Some(0), Some(1), Some(0), Some(1), Some(1)]
    );
}

#[test]
fn moves_every_square_by_the_rows_a_scroll_moved() {
    let whole = Viewport { top: 0, bottom: 63 };
    let placements = placements_of(&rows_for(&pane(), whole), whole);
    let at = |top: i64, height: i64| {
        relative_layout(
            &placements,
            Some(top),
            Viewport {
                top,
                bottom: top + height - 1,
            },
            &[],
            &Options::default(),
        )
    };
    let rows = |strip: &RelativeStrip| -> Vec<(String, f64)> {
        strip
            .squares
            .iter()
            .map(|square| (square.id.clone(), square.y))
            .collect()
    };
    let of = |strip: &RelativeStrip, id: &str| -> Option<f64> {
        strip
            .squares
            .iter()
            .find(|square| square.id == id)
            .map(|square| square.y)
    };

    // The whole pane: every turn is at its own row inside the window.
    let live = at(0, 64);
    assert_eq!(live.squares.len(), 5);
    let start_of = |id: &str| of(&live, id).expect("the live window holds every turn");
    assert!(live.squares.iter().all(|square| (0.0..64.0).contains(&square.y)));

    // Twenty rows up. Every square the window still holds moved by exactly the
    // scroll, none is drawn outside the window, and the turns that scrolled out
    // are gone rather than pinned to the strip's ends.
    let scrolled = at(20, 24);
    assert!(
        scrolled.squares.len() < live.squares.len(),
        "the turns above the window are not drawn: {:?}",
        rows(&scrolled)
    );
    for (id, y) in rows(&scrolled) {
        assert!(
            (0.0..24.0).contains(&y),
            "{id} draws outside the window at {y}"
        );
        assert!(
            close(start_of(&id) - y, 20.0) || y == 0.0,
            "{id} moved {} rows for a 20-row scroll",
            start_of(&id) - y
        );
    }

    // At the bottom of the capture, a turn whose head is above the window draws
    // on row 0 — that is where the reader met it — and the newest sits on its
    // own row.
    let tail = at(40, 24);
    assert_eq!(of(&tail, "t3"), Some(0.0));
    assert!(close(of(&tail, "t5").unwrap(), 61.0 - 40.0));
    assert_eq!(of(&tail, "u1"), None, "a turn above the window is not drawn");
}

#[test]
fn keeps_one_square_per_row_and_the_readers_own_turn_under_a_budget() {
    let whole = Viewport { top: 0, bottom: 59 };
    let rows = rows_for(&many(30), whole);
    let at = |max_squares: usize, focus: i64| {
        layout(
            &rows,
            whole,
            focus,
            &Options {
                max_squares,
                ..Options::default()
            },
        )
    };
    let ids = |strip: &Layout| -> Vec<String> {
        strip
            .squares()
            .iter()
            .map(|square| square.id.clone())
            .collect()
    };

    // Thirty one-line turns two rows apart: the window holds every one of them,
    // because a gap of one row is what a square needs and these have two.
    let uncapped = at(0, 0);
    assert_eq!(uncapped.squares().len(), 30);
    assert_eq!(uncapped.squares()[0].id, "s1");
    assert_eq!(uncapped.squares()[0].y, 0.0);
    assert_eq!(uncapped.squares()[29].y, 58.0);

    // A budget of one keeps the reader's own turn even though it is the oldest,
    // and 24 keeps that turn plus the newest 23.
    assert_eq!(ids(&at(1, 0)), ["s1"]);
    let capped = ids(&at(24, 0));
    assert_eq!(capped.len(), 24);
    assert_eq!(capped[0], "s1", "the reader's turn is never the one dropped");
    let mut newest: Vec<String> = (8..=30).map(|index| format!("s{index}")).collect();
    newest.insert(0, "s1".to_string());
    assert_eq!(capped, newest);
}

#[test]
fn a_crowded_row_gives_its_square_to_the_newer_turn() {
    // Two turns whose heads land on the same row (an estimate can put them
    // there): the newer one draws, the older one is not drawn at all.
    let whole = Viewport { top: 0, bottom: 19 };
    let rows = rows_for(
        &[
            spec("y1", "assistant", (4, 4), 1, &[4]),
            spec("y2", "assistant", (4, 5), 2, &[4, 5]),
        ],
        whole,
    );
    let strip = layout(&rows, whole, 4, &Options::default());
    let ids: Vec<&str> = strip
        .squares()
        .iter()
        .map(|square| square.id.as_str())
        .collect();
    assert_eq!(ids, ["y2"]);
    assert_eq!(strip.squares().len(), 1, "exactly one active square");
    assert!(strip.squares()[0].active);
}

#[test]
fn fits_a_turn_the_matcher_dropped_into_the_gap_its_neighbours_left() {
    let whole = Viewport { top: 0, bottom: 32 };
    let samples = samples_from(&rows_for(&gap(), whole), whole);
    let placements = place_window(
        &window_of(&rows_for(&all(), whole)),
        &samples,
        &measure(&samples, whole),
        whole,
    );
    let expected: [(&str, f64, f64, bool); 3] = [
        ("x1", 0.0, 10.0, true),
        ("x2", 13.17, 6.67, false),
        ("x3", 23.0, 10.0, true),
    ];
    assert_eq!(placements.len(), expected.len());
    for (placement, (id, start, rows, measured)) in placements.iter().zip(expected) {
        assert_eq!(placement.id.as_str(), id);
        assert_eq!(placement.measured, measured);
        assert!(
            close(placement.start, start),
            "{id} start {}",
            placement.start
        );
        assert!(close(placement.rows, rows), "{id} rows {}", placement.rows);
    }
}

/// The strip in relative mode, which is what most of these tests ask for.
fn relative(strip: &Layout) -> &RelativeStrip {
    match strip {
        Layout::Relative(relative) => relative,
        Layout::Map(_) => panic!("relative mode asked for, a map came back"),
    }
}

fn ids_of(strip: &Layout) -> Vec<String> {
    strip
        .squares()
        .iter()
        .map(|square| square.id.clone())
        .collect()
}

#[test]
fn the_maps_squares_do_not_move_on_a_scroll_and_its_block_does() {
    let whole = Viewport { top: 0, bottom: 63 };
    let placements = placements_of(&rows_for(&pane(), whole), whole);
    let options = Options {
        mode: Mode::Map,
        ..Options::default()
    };
    let live = map_layout(&placements, whole, &[], &options);
    let scrolled = map_layout(
        &placements,
        Viewport {
            top: 20,
            bottom: 43,
        },
        &[],
        &options,
    );

    // Every turn the window holds draws, measured or not: that is the point of
    // the map, and it is what a relative strip refuses to do.
    assert_eq!(ids_of(&Layout::Map(live.clone())), ["u1", "a2", "t3", "a4", "t5"]);
    assert!(close(live.span, 58.0), "span {}", live.span);
    assert!(close(live.block.top, 0.0));
    assert!(
        close(live.block.height, live.span),
        "a reader at the live bottom is looking at the whole map: {}",
        live.block.height
    );

    // A scroll moves the block and leaves every square where it was.
    assert_eq!(
        live.squares.iter().map(|square| square.y).collect::<Vec<_>>(),
        scrolled.squares.iter().map(|square| square.y).collect::<Vec<_>>(),
        "the map's squares are the map's, not the reader's"
    );
    assert!(close(scrolled.span, live.span), "a scroll is not a new map");
    assert!(scrolled.block.top > live.block.top, "the block moved down");
    assert!(
        scrolled.block.height < live.block.height,
        "twenty-four rows of a sixty-four-row pane take less of the map"
    );
}

#[test]
fn the_map_caps_its_newest_turns_and_hides_the_tools_on_request() {
    let whole = Viewport { top: 0, bottom: 59 };
    let rows = rows_for(&many(30), whole);
    let options = Options {
        mode: Mode::Map,
        ..Options::default()
    };
    let uncapped = layout(&rows, whole, 0, &options);
    assert_eq!(uncapped.squares().len(), 30);

    let capped = layout(
        &rows,
        whole,
        0,
        &Options {
            max_squares: 4,
            ..options
        },
    );
    assert_eq!(ids_of(&capped), ["s27", "s28", "s29", "s30"]);

    // A hidden tool takes no row of the map either: the squares are the turns
    // that draw, not the rows they used to take.
    let pane_rows = rows_for(&pane(), Viewport { top: 0, bottom: 63 });
    let hidden = layout(
        &pane_rows,
        Viewport { top: 0, bottom: 63 },
        0,
        &Options {
            show_tools: false,
            ..options
        },
    );
    assert_eq!(ids_of(&hidden), ["u1", "a2", "a4"]);
}

#[test]
fn the_band_keeps_the_readers_own_turns_the_mode_placed_nothing_for() {
    let whole = Viewport { top: 0, bottom: 63 };
    let rows = rows_for(&pane(), whole);
    // The reader scrolled past their prompt: `u1` is above the window and a
    // relative strip has no row to give it.
    let viewport = Viewport {
        top: 37,
        bottom: 60,
    };
    let strip = layout_pinned(&rows, &["u1".to_string()], viewport, 37, &Options::default());
    let relative = relative(&strip);
    assert_eq!(relative.band, 1);
    assert_eq!(relative.squares[0].id, "u1");
    assert_eq!(relative.squares[0].y, 0.0, "a band square counts places, not rows");
    assert!(!relative.squares[0].active);
    assert_eq!(relative.squares.len(), 3, "the band plus the window's own two");

    // A pin the window already draws is not drawn twice.
    let doubled = layout_pinned(&rows, &["t3".to_string()], viewport, 37, &Options::default());
    assert_eq!(doubled.band(), 0);

    // The map places its own turns, so nothing is left to pin.
    let mapped = layout_pinned(
        &rows,
        &["u1".to_string()],
        viewport,
        37,
        &Options {
            mode: Mode::Map,
            ..Options::default()
        },
    );
    assert_eq!(mapped.band(), 0);

    // A band shorter than the list keeps the newest of the reader's turns.
    let pins: Vec<String> = ["p1", "p2", "p3"].iter().map(|id| id.to_string()).collect();
    let short = layout_pinned(
        &rows,
        &pins,
        viewport,
        37,
        &Options {
            user_keep: 1,
            ..Options::default()
        },
    );
    assert_eq!(short.band(), 1);
    assert_eq!(short.squares()[0].id, "p3");
}

#[test]
fn the_relative_strips_top_square_is_the_turn_the_reader_is_inside() {
    // An agent turn the reader met mid-way: it draws on the window's first row,
    // because that is the row the reader is reading it on.
    let whole = Viewport { top: 0, bottom: 63 };
    let rows = rows_for(&pane(), whole);
    let strip = layout(
        &rows,
        Viewport {
            top: 40,
            bottom: 63,
        },
        40,
        &Options::default(),
    );
    assert_eq!(strip.squares()[0].id, "t3");
    assert_eq!(strip.squares()[0].y, 0.0);
    assert!(strip.squares()[0].active);

    // The reader's own prompt, the same way: a user turn anchors the top of the
    // strip exactly as an agent turn does.
    let prompt = rows_for(
        &[
            spec("u1", "user", (0, 9), 4, &[0, 2, 4, 6]),
            spec("a2", "assistant", (12, 20), 3, &[12, 15, 18]),
        ],
        Viewport { top: 0, bottom: 23 },
    );
    let strip = layout(
        &prompt,
        Viewport {
            top: 5,
            bottom: 20,
        },
        5,
        &Options::default(),
    );
    assert_eq!(strip.squares()[0].id, "u1");
    assert_eq!(strip.squares()[0].y, 0.0);
    assert!(strip.squares()[0].active);
}

#[test]
fn hiding_the_tools_takes_their_squares_out_of_the_relative_strip() {
    let whole = Viewport { top: 0, bottom: 63 };
    let rows = rows_for(&pane(), whole);
    let shown = layout(&rows, whole, 0, &Options::default());
    assert_eq!(ids_of(&shown), ["u1", "a2", "t3", "a4", "t5"]);
    let hidden = layout(
        &rows,
        whole,
        0,
        &Options {
            show_tools: false,
            ..Options::default()
        },
    );
    assert_eq!(ids_of(&hidden), ["u1", "a2", "a4"]);
    assert!(
        hidden.squares().iter().all(|square| square.kind != TurnKind::Tool),
        "a hidden tool leaves no square behind"
    );
}
