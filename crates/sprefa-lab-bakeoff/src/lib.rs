//! Bakeoff lab: one table, case x tool. Entry shapes per case: `README.md`.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// One tool's answer to one case. `answer` is a canonical, orderless site set.
/// `notes` opening with `cannot` declares the tool has no way to answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseAnswer {
    pub case: String,
    pub answer: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl CaseAnswer {
    /// Orderless view of `answer`; duplicates collapse.
    pub fn set(&self) -> BTreeSet<String> {
        self.answer.iter().cloned().collect()
    }

    pub fn is_cannot(&self) -> bool {
        self.notes
            .as_deref()
            .is_some_and(|n| n.trim_start().to_ascii_lowercase().starts_with("cannot"))
    }
}

/// Column order of the printed table. `ryi-fast` is the baseline column.
pub const TOOLS: [&str; 5] = ["ryi-fast", "aider", "repomix", "continue", "serena"];

/// One scored cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell {
    /// No `out/<tool>/<case>.json` on disk.
    Missing,
    Cannot(String),
    Match,
    Diff {
        extra: Vec<String>,
        missing: Vec<String>,
    },
}

impl Cell {
    pub fn render(&self) -> String {
        match self {
            Cell::Missing => "cannot (no out file)".to_string(),
            Cell::Cannot(why) => format!("cannot: {why}"),
            Cell::Match => "match".to_string(),
            Cell::Diff { extra, missing } => {
                format!("diff +{} -{}", extra.len(), missing.len())
            }
        }
    }
}

/// Set equality wins over a `cannot` note: a tool that answered right scores
/// `match` even if it hedged.
pub fn score(expected: &BTreeSet<String>, got: &CaseAnswer) -> Cell {
    let got_set = got.set();
    if got_set == *expected {
        return Cell::Match;
    }
    if got.is_cannot() {
        return Cell::Cannot(got.notes.clone().unwrap_or_default());
    }
    Cell::Diff {
        extra: got_set.difference(expected).cloned().collect(),
        missing: expected.difference(&got_set).cloned().collect(),
    }
}
