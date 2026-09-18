//! Substring candidates for the exact monotonic matcher. A turn with no
//! matching row cannot enter its dynamic-programming table. Both containment
//! directions index short prefixes, then confirm with the same line predicate
//! as the matcher, including short exact matches and Unicode thresholds.
//! Prefixes bound index construction cost for long tool-output lines.
use aho_corasick::AhoCorasick;

use super::{line_matches, ScreenRow, Source};

pub(super) fn candidates(screen: &[ScreenRow], sources: &[Source]) -> Vec<bool> {
    let mut found = vec![false; sources.len()];
    let rows: Vec<&str> = screen
        .iter()
        .map(|row| row.normalized.as_str())
        .filter(|row| !row.is_empty())
        .collect();
    if rows.is_empty() {
        return found;
    }
    let row_prefixes: Vec<String> = rows
        .iter()
        .map(|row| row.chars().take(12).collect())
        .collect();
    let Ok(row_patterns) = AhoCorasick::new(&row_prefixes) else {
        return vec![true; sources.len()];
    };
    // A screen row contained in a source line, including exact equality.
    for (index, source) in sources.iter().enumerate() {
        found[index] = source.normalized.iter().any(|line| {
            row_patterns
                .find_overlapping_iter(line)
                .any(|hit| line_matches(rows[hit.pattern().as_usize()], line))
        });
    }
    // A source line contained in a screen row. Already found turns need no
    // further candidates. Byte length is a safe containment bound even when
    // the predicate's character lengths differ from bytes.
    let widest = rows.iter().map(|row| row.len()).max().unwrap_or(0);
    let patterns: Vec<(usize, &str)> = sources
        .iter()
        .enumerate()
        .filter(|(index, _)| !found[*index])
        .flat_map(|(index, source)| {
            source
                .normalized
                .iter()
                .filter(move |line| line.len() <= widest)
                .map(move |line| (index, line.as_str()))
        })
        .collect();
    if patterns.is_empty() {
        return found;
    }
    let source_prefixes: Vec<String> = patterns
        .iter()
        .map(|(_, line)| line.chars().take(8).collect())
        .collect();
    let Ok(source_patterns) = AhoCorasick::new(&source_prefixes) else {
        return vec![true; sources.len()];
    };
    for row in rows {
        for hit in source_patterns.find_overlapping_iter(row) {
            let (index, line) = patterns[hit.pattern().as_usize()];
            if !found[index] && line_matches(row, line) {
                found[index] = true;
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BoopTurn, LogicalLine};

    #[test]
    fn candidate_index_matches_pairwise_predicate_in_both_directions() {
        let texts = [
            "",
            "a",
            "short",
            "1234567",
            "12345678",
            "12345678901",
            "123456789012",
            "prefix 12345678 suffix",
            "123456789012 extended",
            "a repeated prefix has different suffix one",
            "a repeated prefix has different suffix two",
            "你好世界你好世界",
            "prefix 你好世界你好世界 suffix",
            "abcdefghijklmnop",
            "abcdefghijklmnopqrstuvxyz",
            "abcdefghijklmnopabcdefghijklmnop",
        ];
        let sources: Vec<Source> = texts
            .iter()
            .enumerate()
            .map(|(index, text)| Source {
                turn: BoopTurn {
                    session: "fixture".into(),
                    harness: "codex".into(),
                    turn: index as i64,
                    ts: index as i64,
                    role: "assistant".into(),
                    said: (*text).into(),
                },
                id: index.to_string(),
                normalized: if text.is_empty() {
                    vec![]
                } else {
                    vec![(*text).into()]
                },
            })
            .collect();
        for left in texts {
            for right in texts {
                let screen: Vec<ScreenRow> = [left, right]
                    .iter()
                    .enumerate()
                    .map(|(index, text)| ScreenRow {
                        line: LogicalLine {
                            text: (*text).into(),
                            start: index,
                            end: index,
                        },
                        normalized: (*text).into(),
                    })
                    .collect();
                let expected: Vec<bool> = sources
                    .iter()
                    .map(|source| {
                        source.normalized.iter().any(|line| {
                            screen.iter().any(|row| {
                                !row.normalized.is_empty() && line_matches(&row.normalized, line)
                            })
                        })
                    })
                    .collect();
                assert_eq!(
                    candidates(&screen, &sources),
                    expected,
                    "screen={left:?}, {right:?}"
                );
            }
        }
    }
}
