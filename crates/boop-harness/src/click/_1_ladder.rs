//! The evidence rung in front of boop-mux's filesystem ladder: a token the
//! pane's agent sessions printed names a file they touched, or one beside it.

use std::path::Path;

use boop_mux::{clean_token, doc_join, doc_roots, resolve_fs, ResolveResult, ResolvedRef, Root, MAX_CHOICES};
use boop_store::SessionTouched;

/// What the pane's agent sessions touched: every path (newest first) and every
/// directory those paths sit under, then each session cwd.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentEvidence {
    pub paths: Vec<String>,
    pub dirs: Vec<String>,
}

impl AgentEvidence {
    pub fn from_touched(touched: &SessionTouched, boundary: &str) -> Self {
        AgentEvidence { dirs: evidence_dirs(&touched.paths, &touched.cwds, boundary), paths: touched.paths.clone() }
    }
}

/// Distinct directories above each touched path up to `boundary`, newest
/// evidence first, then the session cwds. Order is the retry order.
pub fn evidence_dirs(paths: &[String], cwds: &[String], boundary: &str) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    let mut push = |dir: String| {
        if !dir.is_empty() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    };
    let boundary = boundary.trim_end_matches('/');
    for path in paths {
        let mut current = Path::new(path).parent();
        while let Some(dir) = current {
            let text = dir.to_string_lossy();
            if text.len() <= boundary.len() || !text.starts_with(boundary) {
                break;
            }
            push(text.into_owned());
            current = dir.parent();
        }
    }
    for cwd in cwds {
        push(cwd.trim_end_matches('/').to_string());
    }
    dirs
}

/// The rung that knows what the agent did: a token the agent printed names a
/// file it touched, or a file beside one. Exact tail on the touched paths
/// first (newest wins when only one distinct file carries the tail), then the
/// token joined to every directory the evidence sits under.
pub fn resolve_from_evidence(rel: &str, evidence: &AgentEvidence) -> Option<ResolveResult> {
    let tail = rel.trim_start_matches("./").trim_start_matches('/');
    if tail.is_empty() {
        return None;
    }
    let suffix = format!("/{tail}");
    let mut touched: Vec<String> = Vec::new();
    for path in &evidence.paths {
        if (path.ends_with(&suffix) || path == tail) && !touched.contains(path) {
            touched.push(path.clone());
        }
    }
    touched.retain(|path| std::fs::symlink_metadata(path).is_ok());
    if touched.len() == 1 {
        return Some(ResolveResult::Hit {
            reference: ResolvedRef { path: touched.remove(0), line: None, source: "touched" },
        });
    }
    if touched.len() > 1 {
        touched.truncate(MAX_CHOICES);
        return Some(ResolveResult::Choices { paths: touched, line: None, via: "exact", worktrees: Vec::new() });
    }
    for dir in &evidence.dirs {
        let candidate = Path::new(dir).join(tail);
        if std::fs::symlink_metadata(&candidate).is_ok() {
            return Some(ResolveResult::Hit {
                reference: ResolvedRef {
                    path: candidate.to_string_lossy().into_owned(),
                    line: None,
                    source: "touched",
                },
            });
        }
    }
    None
}

/// A token resolved against the click's roots. `roots[0]` is the pane cwd. An
/// absolute token names exactly one path, so the evidence rung only answers
/// relative ones; everything the evidence misses runs the filesystem ladder.
pub fn resolve(token: &str, roots: &[Root], home: &str, evidence: &AgentEvidence) -> ResolveResult {
    if let Some((rel, line)) = clean_token(token) {
        let absolute = rel.starts_with('/') || rel.starts_with("~/");
        if let Some(found) = resolve_from_evidence(&rel, evidence).filter(|_| !absolute) {
            return match found {
                ResolveResult::Hit { reference } => ResolveResult::Hit {
                    reference: ResolvedRef { line, ..reference },
                },
                ResolveResult::Choices { paths, via, worktrees, .. } => ResolveResult::Choices { paths, line, via, worktrees },
                other => other,
            };
        }
    }
    resolve_fs(token, roots, home)
}

/// A token written in the markdown document at `doc`. The document's own
/// roots answer first (`boop_mux::doc_join`); anything else runs the ladder
/// over the document's roots followed by `roots`.
pub fn resolve_in_doc(token: &str, doc: &Path, roots: &[Root], home: &str, evidence: &AgentEvidence) -> ResolveResult {
    doc_join(token, doc).unwrap_or_else(|| resolve(token, &doc_roots(doc, roots), home, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::click::click_roots;
    use boop_mux::PaneHit;
    use std::path::PathBuf;

    fn pane(cwd: &str) -> PaneHit {
        PaneHit { pane: String::new(), pane_current_path: PathBuf::from(cwd), pane_col: 0, pane_row: 0 }
    }

    fn resolve_with(token: &str, cwd: &str, home: &str, evidence: &AgentEvidence) -> ResolveResult {
        let roots = click_roots(&pane(cwd), &SessionTouched::default());
        super::resolve(token, &roots, home, evidence)
    }

    /// RECEIPT. What the agent touched outranks every filesystem guess: the
    /// token joins to a directory above a touched file, so a file the walker
    /// never indexes (gitignored `out/`) resolves from the ledger alone.
    #[test]
    fn a_token_resolves_beside_a_file_the_agent_touched() {
        let root = std::env::temp_dir().join(format!("instant-evidence-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lab = root.join("labs").join("otel");
        std::fs::create_dir_all(lab.join("out")).unwrap();
        std::fs::write(lab.join("out").join("timeline.txt"), "t\n").unwrap();
        std::fs::write(lab.join("out").join("perfetto.png"), "p\n").unwrap();
        let touched = lab.join("out").join("perfetto.png").to_string_lossy().into_owned();
        let boundary = root.to_string_lossy().into_owned();
        let evidence = AgentEvidence {
            dirs: evidence_dirs(&[touched.clone()], &[], &boundary),
            paths: vec![touched.clone()],
        };
        assert_eq!(
            evidence.dirs,
            vec![
                lab.join("out").to_string_lossy().into_owned(),
                lab.to_string_lossy().into_owned(),
                root.join("labs").to_string_lossy().into_owned(),
            ]
        );
        let cwd = boundary.clone();
        let hit = resolve_with("out/timeline.txt:3", &cwd, &boundary, &evidence);
        assert_eq!(
            hit,
            ResolveResult::Hit {
                reference: ResolvedRef {
                    path: lab.join("out").join("timeline.txt").to_string_lossy().into_owned(),
                    line: Some(3),
                    source: "touched",
                }
            }
        );
        let exact = resolve_with("perfetto.png", &cwd, &boundary, &evidence);
        assert_eq!(
            exact,
            ResolveResult::Hit {
                reference: ResolvedRef { path: touched, line: None, source: "touched" }
            }
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
