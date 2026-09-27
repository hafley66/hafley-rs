//! Claude folds tool calls into a rendered summary between conversation
//! blocks. Those rows have no verbatim `said` in the store. Anchor the summary
//! to the last tool in that bounded run; its role stays `tool` and the strip
//! uses it as a separator. No conversation square is synthesized.

use boop_turnvis::{BoopTurn, Confidence, LogicalLine, VisibleTurn};

fn summary(text: &str) -> bool {
    let text = text.trim().to_ascii_lowercase();
    !text.is_empty()
        && text.split(", ").all(|clause| {
            let words: Vec<_> = clause.split_whitespace().collect();
            match words.as_slice() {
                ["read", count, "file" | "files"] => count.parse::<u32>().is_ok(),
                ["ran", count, "shell", "command" | "commands"] => count.parse::<u32>().is_ok(),
                ["called", rest @ ..] if rest.len() >= 3 => {
                    matches!(rest[rest.len() - 1], "time" | "times")
                        && rest[rest.len() - 2].parse::<u32>().is_ok()
                }
                _ => false,
            }
        })
}

/// Match rows through the generic engine with Claude's transcript-shape hook.
/// The hook remains active for mixed-harness inputs and filters every source
/// row by its harness.
pub fn locate_visible_turns(lines: &[LogicalLine], turns: &[BoopTurn]) -> Vec<VisibleTurn> {
    boop_turnvis::locate_visible_turns_with_policy(lines, turns, Some(anchor), accept_turn_match)
}

pub fn accept_turn_match(turn: &BoopTurn, evidence: boop_turnvis::MatchEvidence) -> bool {
    turn.harness != "claude"
        || turn.role != "user"
        || evidence.matched_rows >= 2
        || evidence.has_prompt_marker
}

/// Input keys the claude TUI prints inside `Name(…)`, in the order it prefers them.
const TOOL_HEADER_KEYS: &[&str] = &[
    "command",
    "file_path",
    "notebook_path",
    "path",
    "pattern",
    "url",
    "query",
    "prompt",
];

/// Rows the claude TUI draws for a stored turn besides its `said`: a tool call's
/// `Name(arg)` header (from `arg`, or from an assistant `[Name] {json}` line).
pub fn screen_lines(role: &str, said: &str, arg: Option<&str>) -> Vec<String> {
    match (role, arg) {
        ("tool", Some(arg)) => headers(said, arg),
        ("assistant", _) => said.split('\n').flat_map(json_headers).collect(),
        _ => Vec::new(),
    }
}

/// `Name(arg)` for the arg's first line, plus the `~/…` form the TUI prints for home paths.
fn headers(name: &str, arg: &str) -> Vec<String> {
    let first = arg.split('\n').next().unwrap_or_default();
    let home = home_relative(first).map(|home| format!("{name}({home})"));
    std::iter::once(format!("{name}({first})"))
        .chain(home)
        .collect()
}

/// `[Name] {json}` (the harness reader's tool line) → its headers.
fn json_headers(line: &str) -> Vec<String> {
    let Some((name, json)) = line
        .strip_prefix('[')
        .and_then(|rest| rest.split_once("] "))
    else {
        return Vec::new();
    };
    TOOL_HEADER_KEYS
        .iter()
        .find_map(|key| json_string_field(json, key))
        .map(|arg| headers(name, &arg))
        .unwrap_or_default()
}

/// The first string value of `key` in a JSON object that may be cut short by `cap`.
fn json_string_field(json: &str, key: &str) -> Option<String> {
    let start = json.find(&format!("\"{key}\":\""))? + key.len() + 4;
    let mut out = String::new();
    let mut chars = json[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                other => out.push(other),
            },
            other => out.push(other),
        }
    }
    Some(out)
}

/// `/Users/<name>/…` and `/home/<name>/…` as the TUI prints them: `~/…`.
fn home_relative(path: &str) -> Option<String> {
    let rest = path
        .strip_prefix("/Users/")
        .or_else(|| path.strip_prefix("/home/"))?;
    let (_, tail) = rest.split_once('/')?;
    Some(format!("~/{tail}"))
}

pub fn anchor(lines: &[LogicalLine], turns: &[BoopTurn], visible: &mut Vec<VisibleTurn>) {
    for line in lines.iter().filter(|line| summary(&line.text)) {
        if visible
            .iter()
            .any(|turn| turn.anchor_start <= line.start && line.start <= turn.anchor_end)
        {
            continue;
        }
        let conversation = |turn: &&VisibleTurn| {
            turn.harness == "claude" && matches!(turn.role.as_str(), "user" | "assistant")
        };
        let before = visible
            .iter()
            .filter(conversation)
            .filter(|turn| turn.anchor_end < line.start)
            .max_by_key(|turn| turn.anchor_end);
        let after = visible
            .iter()
            .filter(conversation)
            .filter(|turn| turn.anchor_start > line.end)
            .min_by_key(|turn| turn.anchor_start);
        // Both ends must be visible, and the summary must precede a response.
        // With an open end, a later unseen tool run cannot be distinguished
        // from the run whose summary is on screen.
        let (Some(before), Some(after)) = (before, after) else {
            continue;
        };
        if before.session != after.session || after.role != "assistant" {
            continue;
        }
        let low = before.turn;
        let high = after.turn;
        let Some(tool) = turns
            .iter()
            .filter(|turn| {
                turn.harness == "claude"
                    && turn.session == after.session
                    && turn.role == "tool"
                    && low < turn.turn
                    && turn.turn < high
            })
            .max_by_key(|turn| turn.turn)
        else {
            continue;
        };
        if visible
            .iter()
            .any(|turn| turn.session == tool.session && turn.turn == tool.turn)
        {
            continue;
        }
        visible.push(VisibleTurn {
            session: tool.session.clone(),
            harness: tool.harness.clone(),
            turn: tool.turn,
            ts: tool.ts,
            role: "tool".into(),
            said: tool.said.clone(),
            id: format!("{}:{}", tool.session, tool.turn),
            buffer_start: line.start,
            buffer_end: line.end,
            anchor_start: line.start,
            anchor_end: line.end,
            confidence: Confidence::Anchored,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_tool_summary_needs_harness_and_transcript_evidence() {
        let mut turns = vec![
            BoopTurn {
                session: "fixture".into(),
                harness: "claude".into(),
                turn: 1,
                ts: 1,
                role: "user".into(),
                said: "okay now try".into(),

                aliases: Vec::new(),
            },
            BoopTurn {
                session: "fixture".into(),
                harness: "claude".into(),
                turn: 2,
                ts: 2,
                role: "tool".into(),
                said: "".into(),

                aliases: Vec::new(),
            },
            BoopTurn {
                session: "fixture".into(),
                harness: "claude".into(),
                turn: 3,
                ts: 3,
                role: "assistant".into(),
                said: "Blocked at the extension".into(),

                aliases: Vec::new(),
            },
        ];
        let lines: Vec<_> = [
            "❯ okay now try",
            "",
            "Read 1 file, called bewpp 3 times, ran 1 shell command",
            "",
            "⏺ Blocked at the extension",
        ]
        .iter()
        .enumerate()
        .map(|(index, text)| LogicalLine {
            text: (*text).into(),
            start: index,
            end: index,
        })
        .collect();
        let shape = |turns: &[BoopTurn]| {
            locate_visible_turns(&lines, turns)
                .into_iter()
                .map(|turn| (turn.turn, turn.role, turn.anchor_start, turn.anchor_end))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            shape(&turns),
            [
                (1, "user".into(), 0, 0),
                (2, "tool".into(), 2, 2),
                (3, "assistant".into(), 4, 4)
            ]
        );
        assert_eq!(
            boop_turnvis::locate_visible_turns_with(&lines[2..], &turns, Some(anchor))
                .iter()
                .map(|turn| turn.turn)
                .collect::<Vec<_>>(),
            [3],
            "an open tool run has no defensible aggregate identity"
        );
        assert_eq!(
            boop_turnvis::locate_visible_turns_with(&lines[..3], &turns, Some(anchor))
                .iter()
                .map(|turn| turn.turn)
                .collect::<Vec<_>>(),
            [1]
        );
        turns[1].role = "thinking".into();
        assert_eq!(
            shape(&turns),
            [(1, "user".into(), 0, 0), (3, "assistant".into(), 4, 4)]
        );
        turns[1].role = "tool".into();
        for turn in &mut turns {
            turn.harness = "codex".into();
        }
        assert_eq!(
            shape(&turns),
            [(1, "user".into(), 0, 0), (3, "assistant".into(), 4, 4)]
        );
        assert!(!summary("Read 1 file to understand this example."));
        assert!(!summary("Read a file"));
    }
}
