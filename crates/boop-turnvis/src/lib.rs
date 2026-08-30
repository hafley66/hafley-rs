//! Port of the terminal turn matcher from TypeScript, byte-identical on the
//! golden fixture corpus.

use serde::Deserialize;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    // JavaScript lowercases before stripping; the order is observable.
    let lowered: String = line.to_lowercase();
    let chars: Vec<char> = lowered.chars().collect();
    let mut i = 0;
    while i < chars.len() && is_js_whitespace(chars[i]) {
        i += 1;
    }
    while i < chars.len() && is_leading_marker(chars[i]) {
        i += 1;
    }
    while i < chars.len() && is_js_whitespace(chars[i]) {
        i += 1;
    }
    let mut out = String::with_capacity(chars.len());
    let mut pending_space = false;
    for &c in &chars[i..] {
        if MARKDOWN_DELETE.contains(&c) {
            continue;
        }
        if BORDER_GLYPHS.contains(&c) {
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
        let edge = if step == Step::Down { row.line.end } else { row.line.start };
        let inside = if step == Step::Down { edge <= limit } else { edge >= limit };
        if !inside || row.normalized.is_empty() {
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
            normalized: turn
                .said
                .split('\n')
                .map(normalize_turn_line)
                .filter(|line| !line.is_empty())
                .collect(),
            turn: turn.clone(),
        })
        .collect();

    let mut matches: Vec<TurnMatch> = sources
        .iter()
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

    let mut claimed_rows: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut visible: Vec<VisibleTurn> = Vec::new();
    for m in &matches {
        let unclaimed = m
            .hits
            .iter()
            .filter(|hit| !claimed_rows.contains(&hit.line.start))
            .count();
        if unclaimed * 2 < m.hits.len() {
            continue;
        }
        for hit in &m.hits {
            claimed_rows.insert(hit.line.start);
        }
        let anchor_start = m.hits.iter().map(|hit| hit.line.start).min().unwrap();
        let anchor_end = m.hits.iter().map(|hit| hit.line.end).max().unwrap();
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
