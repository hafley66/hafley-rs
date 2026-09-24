//! Port of the terminal turn matcher from TypeScript, byte-identical on the
//! golden fixture corpus.

use serde::{Deserialize, Serialize};

mod _1_snapshot;
#[path = "0_boop_envelope.rs"]
mod boop_envelope;
pub use boop_envelope::boop_content;
#[path = "0_candidates.rs"]
mod candidates;
pub use _1_snapshot::{
    locate_snapshot_turns, locate_snapshot_turns_with, logical_lines, visible_squares,
    visible_squares_with, TurnSquare, PREVIEW_CHARS,
};

#[derive(Clone, Debug, Deserialize)]
pub struct BoopTurn {
    pub session: String,
    pub harness: String,
    pub turn: i64,
    pub ts: i64,
    pub role: String,
    pub said: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LogicalLine {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum Confidence {
    Anchored,
    Extended,
}

#[derive(Clone, Debug)]
pub struct VisibleTurn {
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

/// Adapter hook for transcript shapes that the generic matcher cannot recover
/// from verbatim source rows.
pub type SummaryAnchor = fn(&[LogicalLine], &[BoopTurn], &mut Vec<VisibleTurn>);

const LEADING_MARKERS: &[char] = &[
    '│', '┃', '┆', '┊', '╎', '╏', '┌', '└', '├', '┬', '╭', '╰', '>', '*', '•', '●', '◉', '⏺', '⏵',
    '◆', '›', '❯', '»', '▶', '🭬', '✨', '✳', '✻', '⎿', '━', '─', '┏', '┓', '┗', '┛', '┠', '┨', '┯',
    '┷', '┼', '╂', '╄', '╅', '╆', '╇', '╈', '╉', '╊', '═', '║', '╔', '╗', '╚', '╝', '╠', '╣', '╦',
    '╩', '╬',
];

const BORDER_GLYPHS: &[char] = &[
    '━', '─', '┏', '┓', '┗', '┛', '┠', '┨', '┯', '┷', '┼', '╂', '╄', '╅', '╆', '╇', '╈', '╉', '╊',
    '═', '║', '╔', '╗', '╚', '╝', '╠', '╣', '╦', '╩', '╬', '|', '│', '┃', '┆', '┊', '╎', '╏', '┌',
    '┐', '└', '┘', '├', '┤', '┬', '┴',
];

const MARKDOWN_DELETE: &[char] = &['`', '_', '*', '~', '#'];

// Matches the JavaScript `\s` class exactly; Rust's char::is_whitespace
// diverges on a few code points (e.g. U+FEFF, U+200B).
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

fn is_leading_marker(c: char) -> bool {
    LEADING_MARKERS.contains(&c)
}

pub fn normalize_turn_line(line: &str) -> String {
    // Envelope grammar is case-sensitive; normalize its body afterwards.
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() && is_js_whitespace(chars[i]) {
        i += 1;
    }
    let had_card_gutter = chars.get(i) == Some(&'│');
    while i < chars.len() && is_leading_marker(chars[i]) {
        i += 1;
    }
    while i < chars.len() && is_js_whitespace(chars[i]) {
        i += 1;
    }
    if had_card_gutter && chars.get(i) == Some(&'$') {
        i += 1;
        while i < chars.len() && is_js_whitespace(chars[i]) {
            i += 1;
        }
    }
    let mut out = String::with_capacity(chars.len());
    let mut pending_space = false;
    let byte_start: usize = chars[..i]
        .iter()
        .map(|character| character.len_utf8())
        .sum();
    for c in boop_content(&line[byte_start..]).to_lowercase().chars() {
        if MARKDOWN_DELETE.contains(&c) {
            continue;
        }
        // Every border glyph except `|` is non-ASCII. Avoid walking the
        // Unicode border table for each ordinary prose character.
        if c == '|' || (!c.is_ascii() && BORDER_GLYPHS.contains(&c)) {
            pending_space = true;
            continue;
        }
        if is_js_whitespace(c) {
            pending_space = true;
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        pending_space = false;
        out.push(c);
    }
    out
}

fn line_matches(screen: &str, source: &str) -> bool {
    let slen = screen.chars().count();
    let src_len = source.chars().count();
    screen == source
        || src_len >= 8
            && ((screen.contains(source) && src_len * 2 >= slen)
                || (source.contains(screen) && slen >= 12))
}

struct Source {
    turn: BoopTurn,
    id: String,
    normalized: Vec<String>,
}

struct ScreenRow {
    line: LogicalLine,
    normalized: String,
}

struct Hit {
    line: LogicalLine,
    source_index: usize,
}

struct TurnMatch {
    source: Source,
    hits: Vec<Hit>,
    source_span: usize,
}

fn monotonic_turn_match(screen: &[ScreenRow], source: &Source) -> Option<TurnMatch> {
    let rows: Vec<&ScreenRow> = screen
        .iter()
        .filter(|row| !row.normalized.is_empty())
        .collect();
    let row_count = rows.len();
    let source_count = source.normalized.len();
    let mut scores = vec![vec![0u32; source_count + 1]; row_count + 1];
    for row in 1..=row_count {
        for column in 1..=source_count {
            let screen_norm = &rows[row - 1].normalized;
            let source_norm = &source.normalized[column - 1];
            let match_score = if line_matches(screen_norm, source_norm) {
                scores[row - 1][column - 1]
                    .saturating_add(1000)
                    .saturating_add(
                        screen_norm.chars().count().min(source_norm.chars().count()) as u32
                    )
            } else {
                0
            };
            scores[row][column] = match_score
                .max(scores[row - 1][column])
                .max(scores[row][column - 1]);
        }
    }
    if scores[row_count][source_count] == 0 {
        return None;
    }
    let mut hits: Vec<Hit> = Vec::new();
    let mut row = row_count;
    let mut column = source_count;
    while row > 0 && column > 0 {
        let screen_norm = &rows[row - 1].normalized;
        let source_norm = &source.normalized[column - 1];
        if line_matches(screen_norm, source_norm)
            && scores[row][column]
                == scores[row - 1][column - 1]
                    .saturating_add(1000)
                    .saturating_add(
                        screen_norm.chars().count().min(source_norm.chars().count()) as u32
                    )
        {
            hits.push(Hit {
                line: rows[row - 1].line.clone(),
                source_index: column - 1,
            });
            row -= 1;
            column -= 1;
        } else if scores[row - 1][column] >= scores[row][column - 1] {
            row -= 1;
        } else {
            column -= 1;
        }
    }
    if hits.is_empty() {
        return None;
    }
    hits.reverse();
    let source_span = hits[hits.len() - 1].source_index - hits[0].source_index + 1;
    Some(TurnMatch {
        source: Source {
            turn: source.turn.clone(),
            id: source.id.clone(),
            normalized: source.normalized.clone(),
        },
        hits,
        source_span,
    })
}

fn source_lines(turn: &BoopTurn) -> Vec<String> {
    if turn.role == "user" {
        return boop_content(&turn.said)
            .split('\n')
            .map(str::to_owned)
            .collect();
    }
    let Some((tool_name, arguments)) = turn.said.split_once('\n') else {
        return turn.said.split('\n').map(str::to_owned).collect();
    };
    if turn.role != "tool" {
        return turn.said.split('\n').map(str::to_owned).collect();
    }
    let Ok(arguments) = serde_json::from_str::<serde_json::Value>(arguments) else {
        return turn.said.split('\n').map(str::to_owned).collect();
    };
    let Some(command) = arguments.get("command").and_then(serde_json::Value::as_str) else {
        return turn.said.split('\n').map(str::to_owned).collect();
    };
    std::iter::once(tool_name.to_owned())
        .chain(command.split('\n').map(str::to_owned))
        .collect()
}

fn match_row_owners(matches: &[TurnMatch]) -> std::collections::HashMap<(usize, String), usize> {
    let mut owners: std::collections::HashMap<(usize, String), std::collections::HashSet<&str>> =
        std::collections::HashMap::new();
    for m in matches {
        for hit in &m.hits {
            owners
                .entry((hit.line.start, m.source.turn.role.clone()))
                .or_default()
                .insert(m.source.id.as_str());
        }
    }
    owners
        .into_iter()
        .map(|(key, sources)| (key, sources.len()))
        .collect()
}

fn has_discriminating_hit(
    hits: &[&Hit],
    screen: &[ScreenRow],
    source: &Source,
    owners: &std::collections::HashMap<(usize, String), usize>,
) -> bool {
    hits.iter().any(|hit| {
        screen
            .iter()
            .find(|row| row.line.start == hit.line.start)
            .is_some_and(|row| {
                let unambiguous =
                    owners.get(&(hit.line.start, source.turn.role.clone())) == Some(&1);
                if source.turn.role == "tool" {
                    unambiguous && row.normalized.chars().count() >= 8
                } else {
                    unambiguous
                        || (source.turn.role == "user"
                            && row.line.text.trim_start().starts_with('❯'))
                }
            })
    })
}

/// A blank row is where one message stops being the other. Extending across one
/// merged two on-screen turns into a single attributed block.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Up,
    Down,
}

fn extend_to(screen: &[ScreenRow], anchor: usize, limit: usize, step: Step) -> usize {
    let Some(at) = screen
        .iter()
        .position(|row| row.line.start <= anchor && anchor <= row.line.end)
    else {
        return anchor;
    };
    let mut reached = anchor;
    let mut index = at as isize;
    loop {
        index += if step == Step::Down { 1 } else { -1 };
        if index < 0 || index as usize >= screen.len() {
            break;
        }
        let row = &screen[index as usize];
        let edge = if step == Step::Down {
            row.line.end
        } else {
            row.line.start
        };
        let inside = if step == Step::Down {
            edge <= limit
        } else {
            edge >= limit
        };
        if !inside || row.normalized.is_empty() || row.normalized == "output" {
            break;
        }
        reached = edge;
    }
    reached
}

fn grow_anchors(visible: &mut [VisibleTurn], screen: &[ScreenRow], sources: &[Source]) {
    let rows: Vec<&ScreenRow> = screen
        .iter()
        .filter(|row| !row.normalized.is_empty())
        .collect();
    let mut owner_at: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    for turn in visible.iter() {
        for row in &rows {
            if row.line.start >= turn.anchor_start && row.line.end <= turn.anchor_end {
                owner_at.insert(row.line.start, turn.id.clone());
            }
        }
    }
    for turn in visible.iter_mut() {
        let id = &turn.id;
        let source = match sources.iter().find(|candidate| &candidate.id == id) {
            Some(source) => source,
            None => continue,
        };
        let claims = |row: &ScreenRow| -> bool {
            let owner = owner_at.get(&row.line.start).unwrap_or(id);
            owner == id
                && source
                    .normalized
                    .iter()
                    .any(|line| line_matches(&row.normalized, line))
        };
        let first = match rows
            .iter()
            .position(|row| row.line.start >= turn.anchor_start)
        {
            Some(first) => first,
            None => continue,
        };
        let mut low = first;
        while low > 0 && claims(rows[low - 1]) {
            low -= 1;
        }
        let mut high = match rows.iter().position(|row| row.line.end >= turn.anchor_end) {
            Some(high) => high,
            None => rows.len() - 1,
        };
        while high + 1 < rows.len() && claims(rows[high + 1]) {
            high += 1;
        }
        turn.anchor_start = turn.anchor_start.min(rows[low].line.start);
        turn.anchor_end = turn.anchor_end.max(rows[high].line.end);
        turn.buffer_start = turn.anchor_start;
        turn.buffer_end = turn.anchor_end;
        for row in &rows[low..=high] {
            owner_at.insert(row.line.start, id.clone());
        }
    }
}

pub fn locate_visible_turns(lines: &[LogicalLine], turns: &[BoopTurn]) -> Vec<VisibleTurn> {
    locate_visible_turns_with(lines, turns, None)
}

/// Match visible rows and let an adapter add bounded transcript evidence before
/// the final non-overlapping buffer extension runs.
pub fn locate_visible_turns_with(
    lines: &[LogicalLine],
    turns: &[BoopTurn],
    summary_anchor: Option<SummaryAnchor>,
) -> Vec<VisibleTurn> {
    let screen: Vec<ScreenRow> = lines
        .iter()
        .map(|line| ScreenRow {
            line: line.clone(),
            normalized: normalize_turn_line(&line.text),
        })
        .collect();
    let sources: Vec<Source> = turns
        .iter()
        .map(|turn| Source {
            id: format!("{}:{}", turn.session, turn.turn),
            normalized: source_lines(turn)
                .iter()
                .map(|line| normalize_turn_line(line))
                .filter(|line| !line.is_empty())
                .collect(),
            turn: turn.clone(),
        })
        .collect();

    let candidates = candidates::candidates(&screen, &sources);
    let mut matches: Vec<TurnMatch> = sources
        .iter()
        .zip(candidates)
        .filter_map(|(source, candidate)| candidate.then_some(source))
        .filter_map(|source| monotonic_turn_match(&screen, source))
        .collect();
    matches.sort_by(|left, right| {
        right
            .hits
            .len()
            .cmp(&left.hits.len())
            .then(left.source_span.cmp(&right.source_span))
            .then(
                left.source
                    .normalized
                    .len()
                    .cmp(&right.source.normalized.len()),
            )
            .then(right.source.turn.ts.cmp(&left.source.turn.ts))
    });

    let row_owners = match_row_owners(&matches);
    let mut claimed_rows: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut visible: Vec<VisibleTurn> = Vec::new();
    for m in &matches {
        let unclaimed: Vec<&Hit> = m
            .hits
            .iter()
            .filter(|hit| !claimed_rows.contains(&hit.line.start))
            .collect();
        if unclaimed.len() * 2 < m.hits.len() {
            continue;
        }
        if m.source.turn.role == "tool"
            && !has_discriminating_hit(&unclaimed, &screen, &m.source, &row_owners)
        {
            continue;
        }
        let anchor_start = unclaimed.iter().map(|hit| hit.line.start).min().unwrap();
        let anchor_end = unclaimed.iter().map(|hit| hit.line.end).max().unwrap();
        if visible
            .iter()
            .any(|turn| anchor_start <= turn.anchor_end && turn.anchor_start <= anchor_end)
        {
            continue;
        }
        for hit in &unclaimed {
            claimed_rows.insert(hit.line.start);
        }
        visible.push(VisibleTurn {
            session: m.source.turn.session.clone(),
            harness: m.source.turn.harness.clone(),
            turn: m.source.turn.turn,
            ts: m.source.turn.ts,
            role: m.source.turn.role.clone(),
            said: m.source.turn.said.clone(),
            id: m.source.id.clone(),
            buffer_start: anchor_start,
            buffer_end: anchor_end,
            anchor_start,
            anchor_end,
            confidence: Confidence::Anchored,
        });
    }
    grow_anchors(&mut visible, &screen, &sources);
    if let Some(summary_anchor) = summary_anchor {
        summary_anchor(lines, turns, &mut visible);
    }
    visible.sort_by(|a, b| {
        a.buffer_start
            .cmp(&b.buffer_start)
            .then(a.turn.cmp(&b.turn))
    });
    if lines.is_empty() {
        return visible;
    }
    let sorted_len = visible.len();
    let mut result: Vec<VisibleTurn> = Vec::with_capacity(sorted_len);
    for (index, turn) in visible.iter().enumerate() {
        let ceiling = if index == 0 {
            lines[0].start
        } else {
            visible[index - 1].buffer_end + 1
        };
        let floor = if index + 1 < sorted_len {
            visible[index + 1].buffer_start.saturating_sub(1)
        } else {
            lines[lines.len() - 1].end
        };
        let buffer_start = extend_to(&screen, turn.buffer_start, ceiling, Step::Up);
        let buffer_end = extend_to(&screen, turn.buffer_end, floor, Step::Down);
        let extended = buffer_start != turn.buffer_start || buffer_end != turn.buffer_end;
        result.push(VisibleTurn {
            buffer_start,
            buffer_end,
            confidence: if extended {
                Confidence::Extended
            } else {
                Confidence::Anchored
            },
            ..turn.clone()
        });
    }
    result
}
