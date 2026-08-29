use boop_turnvis::{locate_visible_turns, BoopTurn, Confidence, LogicalLine, VisibleTurn};
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
struct Golden {
    #[allow(dead_code)]
    fixture: String,
    #[allow(dead_code)]
    cols: u16,
    #[allow(dead_code)]
    rows: u16,
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
        all_failures.extend(compare(name, &got, &golden));
    }
    assert!(all_failures.is_empty(), "\n{}", all_failures.join("\n"));
}
