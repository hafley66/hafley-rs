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
    drawn_as_tool, drawn_at_all, kind_of, layout, layout_pinned, measure, place_window,
    recent_layout, relative_layout, rows_of, samples_from, window_of, Layout, ListedTurn, Mode,
    Options, Placement, RecentStrip, RelativeStrip, TurnKind, TurnRow, Viewport,
    DEFAULT_RECENT_MAX, KINDS,
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

/// `count` tool turns, two rows apart, one line each: the tool budget's
/// fixture. A dense run of tools in one turn is exactly what the strip must
/// compress, so the rows are packed at the tool gap's own distance.
fn many_tools(count: i64) -> Vec<TurnSpec> {
    (0..count)
        .map(|index| {
            spec(
                &format!("t{}", index + 1),
                "tool",
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
    // A window the reader scrolled into the middle of the long answer: `a2`'s
    // head is above it, `a4` sits below it, and the SQL result the matcher saw
    // between them is a tool turn, which draws a tiny square on its own row.
    let window = Viewport {
        top: 25,
        bottom: 58,
    };
    let placements = placements_of(&rows_for(&pane(), window), window);
    let strip = relative_layout(&placements, Some(40), window, &[], &Options::default());
    // `a2` draws on the window's first row because that is where the reader met
    // it, and `a4` on its own row, twenty-seven below. Their sizes are the
    // reader's own intersection with them: the window holds half of `a2`, so
    // half a square; `a4` is wholly in view and draws full size. The tool result
    // between them draws at the fixed tool scale, never active.
    let expected: [(&str, TurnKind, f64, f64, bool); 3] = [
        ("a2", TurnKind::Agent, 0.0, 0.5, true),
        ("t3", TurnKind::Tool, 8.0, 0.35, false),
        ("a4", TurnKind::Agent, 27.0, 1.0, true),
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
fn scales_a_square_by_the_part_of_its_turn_the_reader_can_see() {
    // One six-line answer, one line per row, so `seen` is exactly the fraction
    // of the turn the window holds and the viewport alone decides the size. The
    // floor is out of the way for the two cases below, so they state the
    // intersection rule itself; the default floor's own bite follows.
    let specs = [spec("a1", "assistant", (0, 5), 6, &[0, 1, 2, 3, 4, 5])];
    let bare = Options {
        scale_min: 0.0,
        ..Options::default()
    };
    let one = |viewport: Viewport, options: &Options| -> (String, f64, f64) {
        let placements = placements_of(&rows_for(&specs, viewport), viewport);
        let strip = relative_layout(&placements, Some(viewport.top), viewport, &[], options);
        assert_eq!(strip.squares.len(), 1, "one turn in the window, one square");
        let square = &strip.squares[0];
        (square.id.clone(), square.y, square.scale)
    };

    // Wholly inside the window: the reader can see all of the turn, so it draws
    // full size.
    assert_eq!(
        one(Viewport { top: 0, bottom: 5 }, &bare),
        ("a1".to_string(), 0.0, 1.0),
        "a turn fully in view is full size"
    );

    // Three of the six lines on screen: the square is half a square.
    assert_eq!(
        one(Viewport { top: 0, bottom: 2 }, &bare),
        ("a1".to_string(), 0.0, 0.5),
        "a turn seen half through draws at seen = 0.5"
    );

    // The default floor is 0.4. `a2` seen through a quarter of the window — six
    // of its twenty-four rows — is held up to the floor rather than drawn at a
    // quarter.
    let floored = Options::default();
    assert_eq!(floored.scale_min, 0.4);
    let quarter = Viewport {
        top: 25,
        bottom: 41,
    };
    let placements = placements_of(&rows_for(&pane(), quarter), quarter);
    let strip = relative_layout(&placements, Some(quarter.top), quarter, &[], &floored);
    let a2 = strip
        .squares
        .iter()
        .find(|square| square.id == "a2")
        .expect("a2 draws in this window");
    assert_eq!(a2.scale, 0.4, "the floor is the least a square may draw at");

    // Scrolling the same turn out of the window restates it: `a4` is wholly in
    // view twelve rows into one window and half out of the next, so its square
    // resizes and moves — and it is still `a4`'s square.
    let strip_of = |viewport: Viewport| {
        let placements = placements_of(&rows_for(&pane(), viewport), viewport);
        relative_layout(&placements, Some(viewport.top), viewport, &[], &bare)
    };
    let live = strip_of(Viewport {
        top: 40,
        bottom: 63,
    });
    let live_a4 = live
        .squares
        .iter()
        .find(|square| square.id == "a4")
        .expect("a4 draws in this window");
    assert_eq!(
        (live_a4.id.as_str(), live_a4.y, live_a4.scale),
        ("a4", 12.0, 1.0),
        "the whole answer, on its own row"
    );
    let scrolled = strip_of(Viewport {
        top: 56,
        bottom: 63,
    });
    let scrolled_a4 = scrolled
        .squares
        .iter()
        .find(|square| square.id == "a4")
        .expect("a4 still draws in this window");
    assert_eq!(
        (scrolled_a4.id.as_str(), scrolled_a4.y, scrolled_a4.scale),
        ("a4", 0.0, 0.5),
        "the same turn half out of the window: resized and moved, never renamed"
    );
}

#[test]
fn marks_all_intersections_independently_of_focus() {
    // The same two-square window as the test above.
    let window = Viewport {
        top: 25,
        bottom: 58,
    };
    let placements = placements_of(&rows_for(&pane(), window), window);
    let active = |row: Option<i64>| {
        relative_layout(&placements, row, window, &[], &Options::default())
            .squares
            .iter()
            .filter(|square| square.active)
            .map(|square| square.id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        [
            active(Some(30)),  // inside `a2`, whose head is above the window
            active(Some(56)),  // inside `a4`
            active(Some(0)),   // above every square: the nearest is the first
            active(Some(999)), // below every square: the nearest is the last
            active(Some(40)),  // inside a tool turn, which owns no square
            active(None),      // nothing focused: the end the reader is heading for
        ],
        std::array::from_fn::<_, 6, _>(|_| vec!["a2".to_owned(), "a4".to_owned()])
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

    // The whole pane: every conversation turn is at its own row inside the
    // window, and the two tool turns the pane holds draw as tiny squares beside
    // them.
    let live = at(0, 64);
    assert_eq!(
        rows(&live),
        [
            ("u1".to_string(), 0.0),
            ("a2".to_string(), 3.0),
            ("t3".to_string(), 33.0),
            ("a4".to_string(), 52.0),
            ("t5".to_string(), 61.0),
        ]
    );
    let start_of = |id: &str| of(&live, id).expect("the live window holds every turn");

    // Two rows up: the square the window still holds moved by exactly the
    // scroll, and the turn that scrolled out is gone rather than pinned to the
    // strip's ends.
    let scrolled = at(2, 24);
    assert_eq!(
        rows(&scrolled),
        [("a2".to_string(), 1.0)],
        "the turn above the window is not drawn"
    );
    for (id, y) in rows(&scrolled) {
        assert!(
            (0.0..24.0).contains(&y),
            "{id} draws outside the window at {y}"
        );
        assert!(
            close(start_of(&id) - y, 2.0),
            "{id} moved {} rows for a 2-row scroll",
            start_of(&id) - y
        );
    }
    assert_eq!(
        of(&scrolled, "a4"),
        None,
        "nor is the answer below it, which the matcher did not see here"
    );

    // At the bottom of the capture, the answer the matcher saw sits on its own
    // row, and the tool turn above it draws a tiny square on the window's first
    // row, where the reader met it.
    let tail = at(40, 24);
    assert_eq!(
        of(&tail, "t3"),
        Some(0.0),
        "a tool draws at its first visible row"
    );
    assert_eq!(
        of(&tail, "a2"),
        None,
        "a turn above the window is not drawn"
    );
    assert!(close(
        of(&tail, "a4").expect("a4 is on screen"),
        52.0 - 40.0
    ));
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
    assert_eq!(ids(&at(1, 0)), ids(&uncapped));
    let capped = ids(&at(24, 0));
    assert_eq!(capped.len(), 30);
    assert_eq!(
        capped[0], "s1",
        "the reader's turn is never the one dropped"
    );
    let newest: Vec<String> = (1..=30).map(|index| format!("s{index}")).collect();
    assert_eq!(capped, newest);
}

#[test]
fn a_dense_run_of_tools_compresses_to_the_tool_budget_newest_first() {
    // Thirty tools, two rows apart, all inside the window: a dense run of the
    // kind one turn flies. The strip keeps at most the tool budget of them,
    // newest first, and they are at least the tool gap apart.
    let whole = Viewport { top: 0, bottom: 63 };
    let rows = rows_for(&many_tools(30), whole);
    let strip = layout(&rows, whole, 0, &Options::default());
    let tools = strip
        .squares()
        .iter()
        .filter(|square| square.kind == TurnKind::Tool)
        .collect::<Vec<_>>();
    assert_eq!(tools.len(), 16, "thirty tools compress to the tool budget");
    let y = |index: usize| tools[index].y;
    assert_eq!(y(0), 28.0, "the newest first: the highest rows survive");
    assert_eq!(y(15), 58.0);
    for index in 1..tools.len() {
        assert!(
            (y(index) - y(index - 1)).abs() >= 2.0,
            "tools are at least the tool gap apart"
        );
    }
    for square in tools {
        assert_eq!(
            square.scale, 0.35,
            "a tool square draws at the fixed tool scale"
        );
        assert!(!square.active, "a tool is never active");
    }
}

#[test]
fn a_conversation_square_inside_a_tool_run_keeps_its_own_row_and_gap() {
    // A conversation turn in the middle of a tool run: the tool budget must not
    // evict it, and the conversation's own `min_gap` must still decide whether
    // it draws. Two conversation turns two rows apart (at `min_gap 2.0`) both
    // survive, and the tools around them take the tiny squares on their own
    // rows without evicting either answer.
    let whole = Viewport { top: 0, bottom: 40 };
    let rows = rows_for(
        &[
            spec("t0", "tool", (0, 1), 1, &[0]),
            spec("a1", "assistant", (4, 5), 1, &[4]),
            spec("a2", "assistant", (6, 7), 1, &[6]),
            spec("t3", "tool", (10, 11), 1, &[10]),
            spec("t4", "tool", (12, 13), 1, &[12]),
        ],
        whole,
    );
    let strip = layout(
        &rows,
        whole,
        0,
        &Options {
            min_gap: 2.0,
            ..Options::default()
        },
    );
    let ids = strip
        .squares()
        .iter()
        .map(|square| (square.id.as_str(), square.y))
        .collect::<Vec<_>>();
    // `a1` and `a2` are two rows apart, exactly `min_gap 2.0`, so both clear it
    // and survive: the conversation behavior, untouched by the tools beside
    // them.
    assert_eq!(
        ids,
        [
            ("t0", 0.0),
            ("a1", 4.0),
            ("a2", 6.0),
            ("t3", 10.0),
            ("t4", 12.0)
        ]
    );
}

#[test]
fn recent_mode_lists_no_tool() {
    // The recency list is the conversation's: a chatty tool run takes no place
    // in it, whatever the store hands over.
    let listed = listed_of(&many_tools(10));
    let strip = recent_layout(
        &listed,
        None,
        Viewport { top: 0, bottom: 40 },
        &Options::default(),
    );
    assert_eq!(strip.squares.len(), 0);
    // And a recent strip with a conversation turn lists that turn only, never
    // the tool that flew beside it.
    let mixed = listed_of(&[
        spec("a1", "assistant", (0, 1), 1, &[0]),
        spec("t2", "tool", (2, 3), 1, &[2]),
    ]);
    let strip = recent_layout(
        &mixed,
        None,
        Viewport { top: 0, bottom: 40 },
        &Options::default(),
    );
    assert_eq!(
        strip
            .squares
            .iter()
            .map(|square| square.id.as_str())
            .collect::<Vec<_>>(),
        ["a1"]
    );
}

#[test]
fn overlapping_measured_spans_remain_active() {
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
    assert_eq!(ids, ["y1", "y2"]);
    assert!(strip.squares().iter().all(|square| square.active));
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
        Layout::Recent(_) => panic!("relative mode asked for, a recent strip came back"),
    }
}

/// The strip in recent mode.
fn recent(strip: &Layout) -> &RecentStrip {
    match strip {
        Layout::Recent(recent) => recent,
        Layout::Relative(_) => panic!("recent mode asked for, a relative strip came back"),
    }
}

/// The session's turns as a recency list wants them: id and kind, no rows.
fn listed_of(specs: &[TurnSpec]) -> Vec<ListedTurn> {
    specs
        .iter()
        .map(|spec| ListedTurn {
            id: spec.id.clone(),
            kind: kind_of(spec.role),
        })
        .collect()
}

fn ids_of(strip: &Layout) -> Vec<String> {
    strip
        .squares()
        .iter()
        .map(|square| square.id.clone())
        .collect()
}

#[test]
fn a_recent_strip_lists_its_turns_oldest_first_and_uniform() {
    let listed = listed_of(&pane());
    let viewport = Viewport { top: 0, bottom: 23 };
    let strip = recent_layout(&listed, None, viewport, &Options::default());
    // The pane's tool turns are not the conversation, so the list is the prompt
    // and its two answers: oldest first, one place each, all the same size. No
    // turn is under the reader's row, so the newest square is active — the end
    // the reader is heading for.
    let expected: [(&str, TurnKind, f64, bool); 3] = [
        ("u1", TurnKind::User, 0.0, false),
        ("a2", TurnKind::Agent, 1.0, false),
        ("a4", TurnKind::Agent, 2.0, true),
    ];
    assert_eq!(strip.squares.len(), expected.len());
    for (square, (id, kind, y, active)) in strip.squares.iter().zip(expected) {
        assert_eq!(square.id.as_str(), id);
        assert_eq!(square.kind, kind);
        assert_eq!(square.y, y, "{id} counts places in the block");
        assert_eq!(square.scale, 1.0, "{id} is the size of every other square");
        assert_eq!(square.active, active);
    }
    assert_eq!(
        strip.rows, 24.0,
        "the track, measured as relative measures it"
    );

    // A turn the reader is inside is the active one instead.
    let focused = recent_layout(&listed, Some("a2"), viewport, &Options::default());
    assert_eq!(
        focused.squares.iter().position(|square| square.active),
        Some(1)
    );
}

#[test]
fn a_scroll_leaves_every_recent_place_alone() {
    let whole = Viewport { top: 0, bottom: 63 };
    let rows = rows_for(&pane(), whole);
    let listed = listed_of(&pane());
    let options = Options {
        mode: Mode::Recent,
        ..Options::default()
    };
    let at = |viewport: Viewport, focus_row: i64| {
        let strip = layout_pinned(&rows, &[], &listed, viewport, focus_row, &options);
        recent(&strip).clone()
    };
    let places = |strip: &RecentStrip| -> Vec<(String, f64, f64)> {
        strip
            .squares
            .iter()
            .map(|square| (square.id.clone(), square.y, square.scale))
            .collect()
    };

    // A row inside the second answer, then a scroll into a tool turn the strip
    // does not list: every place is the same place.
    let live = at(whole, 3);
    let scrolled = at(
        Viewport {
            top: 40,
            bottom: 63,
        },
        40,
    );
    assert_eq!(
        places(&live),
        places(&scrolled),
        "a scroll is not a new list"
    );
    // But the track the block is centred in is the window's, and it moved.
    assert_eq!(live.rows, 64.0);
    assert_eq!(scrolled.rows, 24.0);
    // Only `active` moves: the reader met the second answer, scrolled into a
    // tool turn, and is now heading for the newest answer.
    assert_eq!(
        live.squares
            .iter()
            .map(|square| square.active)
            .collect::<Vec<_>>(),
        [true, true, true]
    );
    assert_eq!(
        scrolled.squares.iter().position(|square| square.active),
        Some(2)
    );
}

#[test]
fn the_recent_cap_keeps_the_newest_and_drops_the_oldest() {
    let listed = listed_of(&many(30));
    let viewport = Viewport { top: 0, bottom: 23 };
    let capped = recent_layout(
        &listed,
        None,
        viewport,
        &Options {
            max_squares: 4,
            ..Options::default()
        },
    );
    assert_eq!(
        capped
            .squares
            .iter()
            .map(|square| square.id.as_str())
            .collect::<Vec<_>>(),
        ["s27", "s28", "s29", "s30"],
        "the oldest places fall off, with no marker to say how many"
    );
    assert_eq!(
        capped
            .squares
            .iter()
            .map(|square| square.y)
            .collect::<Vec<_>>(),
        [0.0, 1.0, 2.0, 3.0],
        "the block counts from the oldest place kept"
    );
    assert_eq!(
        capped.squares.iter().position(|square| square.active),
        Some(3)
    );

    // A cap the list does not reach is the whole list; the default `0` is not
    // room, it is the mode's own hard max.
    let roomy = recent_layout(
        &listed,
        None,
        viewport,
        &Options {
            max_squares: 99,
            ..Options::default()
        },
    );
    assert_eq!(roomy.squares.len(), 30);
    assert_eq!(
        recent_layout(&listed, None, viewport, &Options::default())
            .squares
            .len(),
        DEFAULT_RECENT_MAX
    );
}

#[test]
fn a_recent_block_takes_its_hard_max_when_the_caller_sets_none() {
    // Forty turns and a pane of twenty-four rows: the block would not fit, so it
    // stops at the hard max — and the newest turn, the end the reader is heading
    // for, is still there.
    let listed = listed_of(&many(40));
    let strip = recent_layout(
        &listed,
        None,
        Viewport { top: 0, bottom: 23 },
        &Options::default(),
    );
    assert_eq!(strip.squares.len(), DEFAULT_RECENT_MAX);
    assert_eq!(strip.squares.len(), 20);
    assert_eq!(
        strip.squares[0].id, "s21",
        "the oldest of the newest twenty, counting places from zero"
    );
    assert_eq!(strip.squares[0].y, 0.0);
    let newest = strip.squares.last().expect("the block is not empty");
    assert_eq!(
        (newest.id.as_str(), newest.y, newest.active),
        ("s40", 19.0, true)
    );
}

#[test]
fn conversation_draws_in_both_modes_and_tools_only_in_relative() {
    // A tool turn and a meta turn between two answers, one row apart each, with
    // a gap wider than that: a hidden turn that still took a spacing slot would
    // drop the older answer, so both answers surviving is what says the hidden
    // turns cost no slot.
    let whole = Viewport { top: 0, bottom: 10 };
    let specs = [
        spec("a1", "assistant", (7, 7), 1, &[7]),
        spec("t2", "tool", (8, 8), 1, &[8]),
        spec("m3", "meta", (9, 9), 1, &[9]),
        spec("a4", "assistant", (10, 10), 1, &[10]),
    ];
    let rows = rows_for(&specs, whole);
    assert!(drawn_at_all(TurnKind::User));
    assert!(drawn_at_all(TurnKind::Agent));
    assert!(!drawn_at_all(TurnKind::Tool));
    assert!(!drawn_at_all(TurnKind::Other));
    assert!(drawn_as_tool(TurnKind::Tool));
    assert!(!drawn_as_tool(TurnKind::Agent));

    // Relative mode draws the conversation and the tool, but not the meta turn:
    // `m3` is not conversation, and it is not a tool either, so no predicate
    // admits it and it costs no slot.
    let strip = layout(
        &rows,
        whole,
        7,
        &Options {
            min_gap: 2.0,
            ..Options::default()
        },
    );
    assert_eq!(ids_of(&strip), ["a1", "t2", "a4"]);

    // The pane fixture's own window holds two tool turns, and both draw as tiny
    // squares in relative mode.
    let whole_pane = Viewport { top: 0, bottom: 63 };
    let pane_rows = rows_for(&pane(), whole_pane);
    assert_eq!(
        ids_of(&layout(&pane_rows, whole_pane, 0, &Options::default())),
        ["u1", "a2", "t3", "a4", "t5"]
    );

    // The band holds only the conversation: a pin is the reader's own prompt, a
    // place a tool must never take.
    let banded = layout_pinned(
        &rows,
        &["p1".to_string()],
        &[],
        whole,
        7,
        &Options::default(),
    );
    assert!(
        banded.squares()[..banded.band()]
            .iter()
            .all(|square| drawn_at_all(square.kind)),
        "the band holds the conversation too"
    );

    // A recency list arrives whole — the caller's store holds every turn — and
    // the strip is what filters it. Recent mode has no tool squares.
    let listed = listed_of(&specs);
    assert_eq!(listed.len(), 4);
    let listed_strip = recent_layout(&listed, None, whole, &Options::default());
    assert_eq!(
        listed_strip
            .squares
            .iter()
            .map(|square| square.id.as_str())
            .collect::<Vec<_>>(),
        ["a1", "a4"]
    );
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
    let strip = layout_pinned(
        &rows,
        &["u1".to_string()],
        &[],
        viewport,
        37,
        &Options::default(),
    );
    let relative = relative(&strip);
    assert_eq!(relative.band, 1);
    assert_eq!(relative.squares[0].id, "u1");
    assert_eq!(
        relative.squares[0].y, 0.0,
        "a band square counts places, not rows"
    );
    assert!(!relative.squares[0].active);
    assert_eq!(
        relative.squares.len(),
        3,
        "the band, the window's own answer, and the tool the window holds"
    );

    // A pin the window already draws is not drawn twice.
    let doubled = layout_pinned(
        &rows,
        &["a4".to_string()],
        &[],
        viewport,
        37,
        &Options::default(),
    );
    assert_eq!(doubled.band(), 0);

    // A recent strip has no band: its list of the newest turns already holds
    // the reader's own prompts, so a pin beside it would say a turn twice.
    let listed_recent = layout_pinned(
        &rows,
        &["u1".to_string()],
        &listed_of(&pane()),
        viewport,
        37,
        &Options {
            mode: Mode::Recent,
            ..Options::default()
        },
    );
    assert_eq!(listed_recent.band(), 0);
    assert_eq!(recent(&listed_recent).squares.len(), 3);

    // A band shorter than the list keeps the newest of the reader's turns.
    let pins: Vec<String> = ["p1", "p2", "p3"].iter().map(|id| id.to_string()).collect();
    let short = layout_pinned(
        &rows,
        &pins,
        &[],
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
            top: 20,
            bottom: 43,
        },
        20,
        &Options::default(),
    );
    assert_eq!(strip.squares()[0].id, "a2");
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
        Viewport { top: 5, bottom: 20 },
        5,
        &Options::default(),
    );
    assert_eq!(strip.squares()[0].id, "u1");
    assert_eq!(strip.squares()[0].y, 0.0);
    assert!(strip.squares()[0].active);
}
