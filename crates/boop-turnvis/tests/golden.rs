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
    for pair in got.windows(2) {
        if pair[0].buffer_end >= pair[1].buffer_start {
            failures.push(format!(
                "{fixture}: final spans overlap: {} {}..={} and {} {}..={}",
                pair[0].id,
                pair[0].buffer_start,
                pair[0].buffer_end,
                pair[1].id,
                pair[1].buffer_start,
                pair[1].buffer_end,
            ));
        }
    }
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
    "omp-chaotic",
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

fn turn(turn: i64, role: &str, said: &str) -> BoopTurn {
    BoopTurn {
        session: "edge".to_string(),
        harness: "omp".to_string(),
        turn,
        ts: turn,
        role: role.to_string(),
        said: said.to_string(),
    }
}

fn line(text: &str, row: usize) -> LogicalLine {
    LogicalLine {
        text: text.to_string(),
        start: row,
        end: row,
    }
}

#[test]
fn short_unambiguous_non_tool_turn_is_visible() {
    let found = locate_visible_turns(&[line("done", 1)], &[turn(1, "assistant", "done")]);
    assert_eq!(
        found
            .iter()
            .map(|turn| (&turn.id, turn.anchor_start, turn.anchor_end))
            .collect::<Vec<_>>(),
        vec![(&"edge:1".to_string(), 1, 1)]
    );
}

#[test]
fn compact_parent_turn_beats_approval_quoting_the_same_response() {
    let response = "Properties:\n- Every streamed write resets the quiet timer.\n- switchMap cancels the previous wait.\n- No polling.";
    let mut parent = turn(14, "assistant", response);
    parent.session = "parent".to_string();
    parent.ts = 140;
    let mut approval = turn(
        80,
        "assistant",
        &format!(
            "Review the following proposed response:\n<assistant_response>\n{response}\n</assistant_response>\nReturn an approval decision."
        ),
    );
    approval.session = "approval-child".to_string();
    approval.ts = 150;

    let found = locate_visible_turns(
        &[
            line("Properties:", 70),
            line("- Every streamed write resets the quiet timer.", 71),
            line("- switchMap cancels the previous wait.", 72),
            line("- No polling.", 73),
        ],
        &[approval, parent],
    );
    assert_eq!(
        found
            .iter()
            .map(|turn| (&turn.id, turn.buffer_start, turn.buffer_end))
            .collect::<Vec<_>>(),
        vec![(&"parent:14".to_string(), 70, 73)]
    );
}

#[test]
fn repeated_identical_tool_calls_are_unassigned() {
    let said = "bash\n{\"command\":\"echo repeated-tool-command\"}";
    let found = locate_visible_turns(
        &[line("│ $ echo repeated-tool-command", 1)],
        &[turn(1, "tool", said), turn(2, "tool", said)],
    );
    assert!(found.is_empty());
}

#[test]
fn interleaved_anchor_intervals_do_not_overlap() {
    let found = locate_visible_turns(
        &[line("alpha", 1), line("bravo", 2), line("charlie", 3)],
        &[
            turn(1, "assistant", "alpha\ncharlie"),
            turn(2, "assistant", "bravo"),
        ],
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "edge:1");
    assert_eq!((found[0].buffer_start, found[0].buffer_end), (1, 3));
}
