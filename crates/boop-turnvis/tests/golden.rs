use boop_turnvis::{
    locate_visible_turns, normalize_turn_line, BoopTurn, Confidence, LogicalLine, VisibleTurn,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Capture {
    #[allow(dead_code)]
    session: String,
    #[allow(dead_code)]
    cols: u16,
    #[allow(dead_code)]
    rows: u16,
    #[allow(dead_code)]
    bytes: usize,
    lines: Vec<LogicalLine>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoldenTurn {
    id: String,
    #[allow(dead_code)]
    turn: i64,
    #[allow(dead_code)]
    role: String,
    confidence: String,
    anchor_start: usize,
    anchor_end: usize,
    buffer_start: usize,
    buffer_end: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoldenLine {
    start: usize,
    #[allow(dead_code)]
    end: usize,
    normalized: String,
    id: Option<String>,
}

#[derive(Deserialize)]
struct Golden {
    #[allow(dead_code)]
    fixture: String,
    #[allow(dead_code)]
    cols: u16,
    #[allow(dead_code)]
    rows: u16,
    lines: Vec<GoldenLine>,
    turns: Vec<GoldenTurn>,
}

fn confidence_str(c: Confidence) -> &'static str {
    match c {
        Confidence::Anchored => "anchored",
        Confidence::Extended => "extended",
    }
}

fn field_diff(name: &str, fixture: &str, index: usize, got: String, want: String) -> String {
    format!("{fixture}[{index}] {name}: got {got:?}, want {want:?}")
}

const TRUNC: usize = 72;

fn trunc(s: &str) -> String {
    if s.chars().count() <= TRUNC {
        s.to_string()
    } else {
        let cut: String = s.chars().take(TRUNC).collect();
        format!("{cut}...")
    }
}

fn line_diff(
    name: &str,
    fixture: &str,
    index: usize,
    start: usize,
    got: String,
    want: String,
) -> String {
    format!(
        "{fixture} line[{index}] (start {start}) {name}: got {:?}, want {:?}",
        trunc(&got),
        trunc(&want)
    )
}

// Anchor containment, matching the TypeScript located.find accessor order.
fn anchor_id_at(turns: &[VisibleTurn], row: usize) -> Option<&str> {
    turns
        .iter()
        .find(|t| t.anchor_start <= row && row <= t.anchor_end)
        .map(|t| t.id.as_str())
}

fn compare_lines(
    fixture: &str,
    capture: &[LogicalLine],
    got: &[VisibleTurn],
    golden: &Golden,
) -> Vec<String> {
    let mut failures = Vec::new();
    if capture.len() != golden.lines.len() {
        failures.push(format!(
            "{fixture}: line count got {}, want {}",
            capture.len(),
            golden.lines.len()
        ));
    }
    for (index, (line, w)) in capture.iter().zip(golden.lines.iter()).enumerate() {
        let norm = normalize_turn_line(&line.text);
        if norm != w.normalized {
            failures.push(line_diff(
                "normalized",
                fixture,
                index,
                w.start,
                norm,
                w.normalized.clone(),
            ));
        }
        let id = anchor_id_at(got, w.start).map(str::to_string);
        if id != w.id {
            failures.push(line_diff(
                "id",
                fixture,
                index,
                w.start,
                id.unwrap_or_else(|| "null".to_string()),
                w.id.clone().unwrap_or_else(|| "null".to_string()),
            ));
        }
    }
    failures
}

fn compare(fixture: &str, got: &[VisibleTurn], golden: &Golden) -> Vec<String> {
    let mut failures = Vec::new();
    if got.len() != golden.turns.len() {
        failures.push(format!(
            "{fixture}: turn count got {}, want {}",
            got.len(),
            golden.turns.len()
        ));
    }
    for (index, (g, w)) in got.iter().zip(golden.turns.iter()).enumerate() {
        if g.id != w.id {
            failures.push(field_diff("id", fixture, index, g.id.clone(), w.id.clone()));
        }
        if confidence_str(g.confidence) != w.confidence {
            failures.push(field_diff(
                "confidence",
                fixture,
                index,
                confidence_str(g.confidence).to_string(),
                w.confidence.clone(),
            ));
        }
        for (name, a, b) in [
            ("anchorStart", g.anchor_start, w.anchor_start),
            ("anchorEnd", g.anchor_end, w.anchor_end),
            ("bufferStart", g.buffer_start, w.buffer_start),
            ("bufferEnd", g.buffer_end, w.buffer_end),
        ] {
            if a != b {
                failures.push(field_diff(
                    name,
                    fixture,
                    index,
                    a.to_string(),
                    b.to_string(),
                ));
            }
        }
    }
    failures
}

const FIXTURES: &[&str] = &[
    "claude",
    "claude-wide",
    "claude-narrow",
    "codex",
    "ccz",
    "opencode",
    "kimi",
];

fn load<T: for<'de> Deserialize<'de>>(path: &str) -> T {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

#[test]
fn golden_fixtures() {
    let dir = env!("CARGO_MANIFEST_DIR").to_string() + "/tests/fixtures";
    let mut all_failures = Vec::new();
    for name in FIXTURES {
        let capture: Capture = load(&format!("{dir}/{name}.json"));
        let turns_name = name
            .strip_suffix("-wide")
            .or_else(|| name.strip_suffix("-narrow"))
            .unwrap_or(name);
        let turns: Vec<BoopTurn> = load(&format!("{dir}/{turns_name}.turns.json"));
        let golden: Golden = load(&format!("{dir}/{name}.golden.json"));
        let got = locate_visible_turns(&capture.lines, &turns);
        all_failures.extend(compare_lines(name, &capture.lines, &got, &golden));
        all_failures.extend(compare(name, &got, &golden));
    }
    assert!(all_failures.is_empty(), "\n{}", all_failures.join("\n"));
}
