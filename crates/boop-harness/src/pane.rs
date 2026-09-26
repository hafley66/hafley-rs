//! The one projection of a session's turns onto a pane: boop-mux snapshot + store turns -> PaneFrame.
//! The CLI prints it, instant pushes it; fields cross JSON under their Rust names.

use std::collections::{BTreeMap, HashSet};

use anyhow::{Context, Result};
use boop_mux::{Multiplexer, TerminalSnapshot, Viewport};
use boop_store::ident::{Store, TurnQuery};
use boop_store::rows::TurnRow;
use boop_turnstrip::{drawn_at_all, kind_of, ListedTurn, TurnKind};
pub use boop_turnstrip::{Layout, Mode, Options};
use serde::{Deserialize, Serialize};

use crate::harness::claude_summary::locate_visible_turns;
use crate::Registry;

/// History rows a frame reads above the reader's window.
pub const HISTORY_ROWS: u32 = 400;
/// Turns a frame reads back from the session's newest.
pub const TURN_WINDOW: u64 = 300;
/// How deep the recency list reads. The conversation kinds are filtered out
/// first, so a long run of tool turns costs the list nothing.
const RECENT_POOL: usize = 200;

/// `pinned`: the reader's prompt above the capture. `listed`: placed by the
/// recency list, never matched. Neither has rows.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    anchored,
    extended,
    pinned,
    listed,
}

/// One turn and the capture rows it holds, inclusive.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlacedTurn {
    pub session: String,
    pub harness: String,
    pub turn: i64,
    pub ts: i64,
    pub role: String,
    pub said: String,
    pub id: String,
    pub buffer_start: usize,
    pub buffer_end: usize,
    pub anchor_start: usize,
    pub anchor_end: usize,
    pub confidence: Confidence,
}

/// A client keeps the newest `at` per pane. A turn's screen row is
/// `buffer_start - window.top`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaneFrame {
    pub pane: String,
    pub session: String,
    pub at: i64,
    pub rows: usize,
    pub turns: Vec<PlacedTurn>,
    pub pinned: Vec<PlacedTurn>,
    pub tags: BTreeMap<String, Vec<String>>,
    pub layout: Option<Layout>,
    pub window: Option<Viewport>,
}

/// Read the pane and the store once and build the frame.
pub fn frame(
    mux: &dyn Multiplexer,
    socket: Option<&str>,
    target: &str,
    store: &Store,
    session: &str,
    options: &Options,
) -> Result<PaneFrame> {
    let snapshot = mux
        .pane_snapshot(socket, target, HISTORY_ROWS)
        .with_context(|| format!("pane {target} answered no snapshot"))?;
    let turns = current_conversation(turns(store, session)?, reset_from(store, session)?);
    let frame = project(&snapshot, session, turns, options);
    // The band's turns carry marks too: a pinned prompt is the square a reader
    // hovers to see what they asked.
    let sources: Vec<String> = frame.turns.iter().chain(&frame.pinned).map(source_of).collect();
    let tags = store.tags_for_many(&sources)?;
    Ok(PaneFrame { tags, ..frame })
}

/// The session's newest `TURN_WINDOW` turns, oldest first, with presentation
/// roles resolved (see [`classify`]).
pub fn turns(store: &Store, session: &str) -> Result<Vec<TurnRow>> {
    let query = TurnQuery {
        session: Some(session.to_owned()),
        ..Default::default()
    };
    let mut rows = store.turn_rows_recent(&query, TURN_WINDOW)?;
    classify(store, &mut rows)?;
    Ok(rows)
}

/// The projection: snapshot and turns in, spans and the strip's geometry out.
/// Pure, so tests pin it without tmux; tags are the caller's read.
pub fn project(snapshot: &TerminalSnapshot, session: &str, turns: Vec<TurnRow>, options: &Options) -> PaneFrame {
    let rows: Vec<&str> = snapshot.rows.iter().map(|row| row.text.as_str()).collect();
    let harness = turns.iter().max_by_key(|turn| turn.ts).map(|turn| turn.harness.as_str());
    let registry = Registry::discover();
    let composer = harness
        .and_then(|name| registry.by_name(name))
        .and_then(|adapter| adapter.terminal_input_region(&rows));
    // One logical line per physical row: a row's index is the row the pane
    // draws it on, so squares line up with rows.
    let lines: Vec<boop_turnvis::LogicalLine> = rows
        .iter()
        .enumerate()
        .filter(|(index, _)| match &composer {
            Some(region) => *index < region.start || *index > region.end,
            None => true,
        })
        .map(|(index, text)| boop_turnvis::LogicalLine {
            text: (*text).to_owned(),
            start: index,
            end: index,
        })
        .collect();
    let located = locate_visible_turns(&lines, &turns.iter().map(to_turnvis).collect::<Vec<_>>());
    let pins = pinned_of(&turns, options);
    let pin_ids: Vec<String> = pins.iter().map(|turn| turn_id(turn)).collect();
    let listed = listed_of(&turns);
    let window = snapshot.window();
    let layout = window.map(|viewport| {
        let turn_rows: Vec<boop_turnstrip::TurnRow> = located
            .iter()
            .map(|turn| boop_turnstrip::rows_of(&lines, turn, viewport))
            .collect();
        boop_turnstrip::layout_pinned(&turn_rows, &pin_ids, &listed, viewport, viewport.top, options)
    });
    // Only the pins the strip drew ride the frame: the crate drops a pin the
    // mode placed a square for.
    let band: HashSet<&str> = match &layout {
        Some(layout) => layout.squares()[..layout.band()].iter().map(|square| square.id.as_str()).collect(),
        None => HashSet::new(),
    };
    let pinned: Vec<PlacedTurn> = pins
        .iter()
        .filter(|turn| band.contains(turn_id(turn).as_str()))
        .map(|turn| unplaced(turn, Confidence::pinned))
        .collect();
    let mut shipped: Vec<PlacedTurn> = located.into_iter().map(placed).collect();
    // Recent mode places turns the window lost; the client drops a square whose turn it cannot name.
    if options.mode == Mode::Recent {
        if let Some(layout) = &layout {
            let carried: HashSet<String> = shipped.iter().map(|turn| turn.id.clone()).collect();
            for turn in &turns {
                let id = turn_id(turn);
                if layout.squares().iter().any(|square| square.id == id) && !carried.contains(&id) {
                    shipped.push(unplaced(turn, Confidence::listed));
                }
            }
        }
    }
    PaneFrame {
        pane: snapshot.target.terminal.clone(),
        session: session.to_owned(),
        at: now_ms(),
        rows: rows.len(),
        turns: shipped,
        pinned,
        tags: BTreeMap::new(),
        layout,
        window,
    }
}

/// What a client draws, as one string. Text *length* only (a streaming turn only
/// grows); `at` excluded (every frame has a fresh one).
pub fn fingerprint(frame: &PaneFrame) -> String {
    let mut out = format!("{}|{}|rows={}|", frame.pane, frame.session, frame.rows);
    if let Some(layout) = &frame.layout {
        out.push_str(&serde_json::to_string(layout).unwrap_or_default());
    }
    out.push_str(&format!("|{:?}|", frame.window));
    for turn in frame.turns.iter().chain(&frame.pinned) {
        out.push_str(&format!(
            "{}:{}:{}:{}:{}:{}:{}:{:?};",
            turn.id, turn.role, turn.ts, turn.said.len(), turn.buffer_start, turn.anchor_start, turn.anchor_end, turn.confidence,
        ));
    }
    out.push('|');
    for (source, tags) in &frame.tags {
        out.push_str(&format!("{source}={};", tags.join(",")));
    }
    out
}

/// The tag source a turn's marks are stored under.
pub fn source_of(turn: &PlacedTurn) -> String {
    format!("turn:{}:{}", turn.session, turn.turn)
}

fn turn_id(turn: &TurnRow) -> String {
    format!("{}:{}", turn.session, turn.turn)
}

fn to_turnvis(turn: &TurnRow) -> boop_turnvis::BoopTurn {
    boop_turnvis::BoopTurn {
        session: turn.session.clone(),
        harness: turn.harness.clone(),
        turn: turn.turn,
        ts: turn.ts,
        role: turn.role.clone(),
        said: turn.said.clone(),
    }
}

fn placed(found: boop_turnvis::VisibleTurn) -> PlacedTurn {
    PlacedTurn {
        session: found.session,
        harness: found.harness,
        turn: found.turn,
        ts: found.ts,
        role: found.role,
        said: found.said,
        id: found.id,
        buffer_start: found.buffer_start,
        buffer_end: found.buffer_end,
        anchor_start: found.anchor_start,
        anchor_end: found.anchor_end,
        confidence: match found.confidence {
            boop_turnvis::Confidence::Anchored => Confidence::anchored,
            boop_turnvis::Confidence::Extended => Confidence::extended,
        },
    }
}

/// A turn on the frame with no rows: its span is the zero it never had.
fn unplaced(turn: &TurnRow, confidence: Confidence) -> PlacedTurn {
    PlacedTurn {
        session: turn.session.clone(),
        harness: turn.harness.clone(),
        turn: turn.turn,
        ts: turn.ts,
        role: turn.role.clone(),
        said: turn.said.clone(),
        id: turn_id(turn),
        buffer_start: 0,
        buffer_end: 0,
        anchor_start: 0,
        anchor_end: 0,
        confidence,
    }
}

/// The reader's own turns, newest `user_keep`, oldest first.
fn pinned_of<'a>(turns: &'a [TurnRow], options: &Options) -> Vec<&'a TurnRow> {
    if options.user_keep == 0 {
        return Vec::new();
    }
    let mut kept: Vec<&TurnRow> = turns.iter().filter(|turn| kind_of(&turn.role) == TurnKind::User).collect();
    kept.sort_by_key(|turn| (turn.ts, turn.turn));
    if kept.len() > options.user_keep {
        kept.drain(..kept.len() - options.user_keep);
    }
    kept
}

/// The recency list's members: the newest `RECENT_POOL` conversation turns,
/// oldest first, from the store, so a turn the window lost still counts.
fn listed_of(turns: &[TurnRow]) -> Vec<ListedTurn> {
    let mut drawn: Vec<&TurnRow> = turns.iter().filter(|turn| drawn_at_all(kind_of(&turn.role))).collect();
    drawn.sort_by_key(|turn| (turn.ts, turn.turn));
    if drawn.len() > RECENT_POOL {
        drawn.drain(..drawn.len() - RECENT_POOL);
    }
    drawn
        .into_iter()
        .map(|turn| ListedTurn { id: turn_id(turn), kind: kind_of(&turn.role) })
        .collect()
}

/// When the session's conversation was last dropped, in turn milliseconds.
fn reset_from(store: &Store, session: &str) -> Result<Option<i64>> {
    Ok(store
        .session_attr(session, boop_store::RESET_ATTR_KEY)?
        .and_then(|value| value.parse::<i64>().ok()))
}

/// The turns after the reset boundary (omp `/clear` keeps the session and its
/// turns). A turn stamped at the boundary belongs to the dropped side.
fn current_conversation(turns: Vec<TurnRow>, boundary: Option<i64>) -> Vec<TurnRow> {
    match boundary {
        Some(boundary) => turns.into_iter().filter(|turn| turn.ts > boundary).collect(),
        None => turns,
    }
}

/// Presentation roles, ledger untouched: a boop-envelope-only user row is `meta`
/// (envelope stripped), a claude usage-only response row is `thinking`.
pub fn classify(store: &Store, rows: &mut [TurnRow]) -> Result<()> {
    for row in rows.iter_mut().filter(|row| row.role == "user") {
        let content = boop_turnvis::boop_content(&row.said);
        if content.len() != row.said.len() {
            if content.is_empty() {
                row.role = "meta".into();
            }
            row.said = content.to_owned();
        }
    }
    let candidates: Vec<(String, i64)> = rows
        .iter()
        .filter(|row| row.harness == "claude" && row.role == "assistant" && row.said.is_empty())
        .map(|row| (row.session.clone(), row.turn))
        .collect();
    if candidates.is_empty() {
        return Ok(());
    }
    let values = (0..candidates.len())
        .map(|index| format!("(?{},?{})", index * 2 + 1, index * 2 + 2))
        .collect::<Vec<_>>()
        .join(",");
    let params: Vec<rusqlite::types::Value> = candidates
        .iter()
        .flat_map(|(session, turn)| [rusqlite::types::Value::Text(session.clone()), rusqlite::types::Value::Integer(*turn)])
        .collect();
    let mut statement = store.connection().prepare(&format!(
        "WITH candidate(session,turn) AS (VALUES {values})
         SELECT candidate.session,candidate.turn FROM candidate
         JOIN dict_session s ON s.value=candidate.session
         JOIN agent_usage u ON u.session_id=s.id AND u.turn=candidate.turn"
    ))?;
    let usage_only: HashSet<(String, i64)> = statement
        .query_map(rusqlite::params_from_iter(params.iter()), |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for row in rows {
        if usage_only.contains(&(row.session.clone(), row.turn)) {
            row.role = "thinking".to_owned();
        }
    }
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use boop_mux::{rows_from_capture, History, Screen, TerminalSize, TerminalTarget};

    fn turn(session: &str, index: i64, role: &str, said: &str) -> TurnRow {
        TurnRow {
            session: session.to_owned(),
            harness: "claude".to_owned(),
            turn: index,
            ts: 1_700_000_000_000 + index,
            role: role.to_owned(),
            said: said.to_owned(),
        }
    }

    /// A pane of `height` rows whose capture is `rows`, scrolled `scroll` up.
    fn pane(rows: &[&str], height: u16, scroll: u32) -> TerminalSnapshot {
        let text: String = rows.iter().map(|row| format!("{row}\n")).collect();
        let size = TerminalSize { columns: 80, rows: height };
        TerminalSnapshot {
            target: TerminalTarget { host: "tmux".into(), terminal: "%1".into(), incarnation: 1 },
            generation: 0,
            size,
            screen: Screen::Primary,
            history: History::Retained { rows: rows.len() as u32, capacity: 2000 },
            cursor: None,
            scroll,
            rows: rows_from_capture(&text, &text, size),
        }
    }

    fn sources(turns: &[PlacedTurn]) -> Vec<String> {
        turns.iter().map(source_of).collect()
    }

    #[test]
    fn a_capture_becomes_spans_in_one_frame() {
        let snapshot = pane(
            &["❯ add the batch verb", "", "⏺ Reading the store", "", "  crates/boop-store/src/tags.rs"],
            3,
            0,
        );
        let turns = vec![turn("s1", 1, "user", "add the batch verb"), turn("s1", 2, "assistant", "Reading the store")];
        let frame = project(&snapshot, "s1", turns, &Options::default());

        assert_eq!((frame.pane.as_str(), frame.session.as_str(), frame.rows), ("%1", "s1", 5));
        assert_eq!(sources(&frame.turns), ["turn:s1:1", "turn:s1:2"]);
        for found in &frame.turns {
            assert!(found.buffer_start <= found.buffer_end && found.buffer_end < frame.rows, "{found:?}");
        }
        assert_eq!(frame.window, Some(Viewport { top: 2, bottom: 4 }));
        let layout = frame.layout.expect("a window means a layout");
        assert_eq!(layout.squares().iter().filter(|square| square.active).count(), 1, "{:?}", layout.squares());
        // The prompt on row 0 is above the three-row window: it is the band's.
        assert_eq!(layout.band(), 1, "{:?}", layout.squares());
        assert_eq!(layout.squares()[0].id, "s1:1");
        assert_eq!(sources(&frame.pinned), ["turn:s1:1"]);
        let drawn: Vec<&str> = layout.squares()[layout.band()..].iter().map(|square| square.id.as_str()).collect();
        assert_eq!(drawn, ["s1:2"]);
        let Layout::Relative(relative) = layout else {
            panic!("relative unless the reader asked otherwise");
        };
        assert_eq!(relative.rows, 3.0);
    }

    #[test]
    fn a_blank_pane_is_one_frame_with_nothing_on_it() {
        let frame = project(&pane(&[], 10, 0), "s1", Vec::new(), &Options::default());
        assert_eq!(frame.rows, 10, "a pane is its rows, written or not");
        assert!(frame.turns.is_empty() && frame.pinned.is_empty() && frame.tags.is_empty());
    }

    #[test]
    fn a_prompt_above_the_capture_rides_the_frame_as_a_pin() {
        let snapshot = pane(&["⏺ done", "", "❯ and again", "", "⏺ working"], 5, 0);
        let turns = vec![
            turn("s1", 1, "user", "the prompt nobody can see any more"),
            turn("s1", 2, "assistant", "done"),
            turn("s1", 3, "user", "and again"),
            turn("s1", 4, "assistant", "working"),
        ];
        let frame = project(&snapshot, "s1", turns, &Options::default());
        assert_eq!(sources(&frame.pinned), ["turn:s1:1"], "{:?}", frame.pinned);
        assert_eq!(frame.pinned[0].confidence, Confidence::pinned);
        assert_eq!(frame.pinned[0].buffer_start, 0);
        let layout = frame.layout.expect("layout");
        assert_eq!(layout.band(), 1);
        assert_eq!(layout.squares()[0].id, "s1:1");
        assert_eq!(layout.squares()[0].y, 0.0, "a band square counts places, not rows");
        assert!(!layout.squares()[0].active);
        assert!(!layout.squares()[1..].iter().any(|square| square.id == "s1:1"), "a pin is not drawn twice");
    }

    #[test]
    fn the_composer_is_not_a_turn() {
        let snapshot = pane(&["❯ hi", "", "⏺ done", "", "╭──────╮", "│ ❯    │", "╰──────╯"], 4, 0);
        let frame = project(&snapshot, "s1", vec![turn("s1", 1, "assistant", "done")], &Options::default());
        for found in &frame.turns {
            assert!(found.buffer_end < 5, "no span may reach the composer: {found:?}");
        }
    }

    #[test]
    fn the_window_follows_the_pane_height_and_its_scroll() {
        let rows = [
            "alpha one", "alpha two", "alpha three", "alpha four", "beta one", "beta two", "beta three", "beta four",
        ];
        let turns = vec![
            turn("s1", 1, "user", "alpha one\nalpha two\nalpha three\nalpha four"),
            turn("s1", 2, "assistant", "beta one\nbeta two\nbeta three\nbeta four"),
            // Never on the pane: only the recency list knows it.
            turn("s1", 3, "assistant", "a reply the capture never held"),
        ];
        let at = |height: u16, scroll: u32, options: &Options| project(&pane(&rows, height, scroll), "s1", turns.clone(), options);
        let ids = |frame: &PaneFrame| -> Vec<String> {
            let layout = frame.layout.as_ref().expect("layout");
            layout.squares()[layout.band()..].iter().map(|square| square.id.clone()).collect()
        };
        let active = |frame: &PaneFrame| -> Option<String> {
            frame.layout.as_ref().expect("layout").squares().iter().find(|square| square.active).map(|square| square.id.clone())
        };
        let relative = Options::default();
        let (whole, tail, scrolled) = (at(8, 0, &relative), at(4, 0, &relative), at(4, 4, &relative));
        assert_eq!(whole.turns.len(), 2, "{:?}", whole.turns);
        assert_eq!(ids(&whole), ["s1:1", "s1:2"]);
        assert_eq!(ids(&tail), ["s1:2"]);
        assert_eq!(ids(&scrolled), ["s1:1"]);
        assert_eq!(
            (active(&whole).as_deref(), active(&tail).as_deref(), active(&scrolled).as_deref()),
            (Some("s1:1"), Some("s1:2"), Some("s1:1"))
        );
        assert_eq!(tail.layout.as_ref().unwrap().band(), 1, "the prompt above the tail is the band's");

        let recent = Options { mode: Mode::Recent, ..Options::default() };
        let (whole, tail, scrolled) = (at(8, 0, &recent), at(4, 0, &recent), at(4, 4, &recent));
        let places = |frame: &PaneFrame| -> Vec<(String, f64)> {
            let Some(Layout::Recent(recent)) = &frame.layout else {
                panic!("recent mode asked for: {:?}", frame.layout);
            };
            recent.squares.iter().map(|square| (square.id.clone(), square.y)).collect()
        };
        assert_eq!(
            places(&whole),
            [("s1:1".to_owned(), 0.0), ("s1:2".to_owned(), 1.0), ("s1:3".to_owned(), 2.0)]
        );
        assert_eq!(places(&tail), places(&whole), "a scroll moves no square");
        assert_eq!(places(&scrolled), places(&whole));
        assert_eq!(scrolled.layout.as_ref().unwrap().band(), 0);
        let shipped: Vec<(&str, Confidence)> = scrolled.turns.iter().map(|turn| (turn.id.as_str(), turn.confidence)).collect();
        assert_eq!(shipped.iter().map(|(id, _)| *id).collect::<Vec<_>>(), ["s1:1", "s1:2", "s1:3"]);
        assert_eq!(shipped[2].1, Confidence::listed, "{shipped:?}");
    }

    #[test]
    fn a_cleared_conversation_stops_at_its_boundary() {
        let turns = vec![
            turn("s1", 1, "user", "the cleared prompt"),
            turn("s1", 2, "assistant", "the cleared answer"),
            turn("s1", 3, "user", "after the clear"),
        ];
        let kept: Vec<i64> = current_conversation(turns.clone(), Some(1_700_000_000_000 + 2)).iter().map(|turn| turn.turn).collect();
        assert_eq!(kept, [3]);
        assert_eq!(current_conversation(turns, None).len(), 3);
    }

    #[test]
    fn a_frame_is_unchanged_only_when_what_the_client_draws_is() {
        let snapshot = pane(&["❯ hi", "", "⏺ done"], 3, 0);
        let frame = |said: &str| {
            project(&snapshot, "s1", vec![turn("s1", 1, "user", "hi"), turn("s1", 2, "assistant", said)], &Options::default())
        };
        let one = frame("done");
        assert_eq!(fingerprint(&one), fingerprint(&frame("done")));
        assert_ne!(fingerprint(&one), fingerprint(&frame("done, and then some more")), "a turn that grew");
        let mut later = frame("done");
        later.at += 5_000;
        assert_eq!(fingerprint(&one), fingerprint(&later), "the stamp is not a reason to redraw");
        let mut resized = frame("done");
        resized.rows += 1;
        assert_ne!(fingerprint(&one), fingerprint(&resized));
    }

    #[test]
    fn a_frame_crosses_json_under_its_rust_names() {
        let snapshot = pane(&["❯ hi", "", "⏺ done"], 3, 0);
        let frame = project(&snapshot, "s1", vec![turn("s1", 1, "user", "hi"), turn("s1", 2, "assistant", "done")], &Options::default());
        let json = serde_json::to_value(&frame).unwrap();
        let turn = &json["turns"][0];
        assert!(turn.get("buffer_start").is_some() && turn.get("bufferStart").is_none(), "{turn}");
        assert_eq!(turn["confidence"], "anchored");
        assert_eq!(json["window"], serde_json::json!({ "top": 0, "bottom": 2 }));
    }
}
