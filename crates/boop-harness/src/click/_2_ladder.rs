//! The ⌘-click ladder over click roots: absolute, what the agent touched, joins
//! onto each root, worktree fan-out, the index, siblings, fzf, then git history.

use std::path::Path;

use boop_store::SessionTouched;
use serde::Serialize;

use super::_0_rungs::*;
use super::_1_roots::{doc_roots, Root, RootVia};

#[derive(Serialize, Debug, PartialEq, Eq, Clone)]
pub struct ResolvedRef {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub source: &'static str,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ResolveResult {
    Hit {
        #[serde(rename = "ref")]
        reference: ResolvedRef,
    },
    Choices {
        paths: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
        via: &'static str,
        /// Parallel to `paths` when `via` is `worktree`: the branch each path is on.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        worktrees: Vec<String>,
    },
    Absent {
        repo: String,
        rev: String,
        path: String,
        subject: String,
    },
    Miss,
}

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

fn join(dir: &Path, tail: &str) -> String {
    format!("{}/{}", trim_slash(&dir.to_string_lossy()), tail)
}

/// The rung label a join onto `via` answers with.
fn source_of(via: &RootVia) -> &'static str {
    match via {
        RootVia::Document => "doc",
        RootVia::PaneCwd => "cwd",
        RootVia::SessionCwd => "session",
        RootVia::GitToplevel => "repo",
        RootVia::Worktree(_) => "worktree",
        RootVia::Touched => "touched",
    }
}

/// Joins in order: the pane's own roots (cwd, session cwds, checkouts), then
/// ancestors up to `home`. The first path on disk wins.
fn local_join(rel: &str, roots: &[Root], cwd: &str, repo_root: Option<&str>, home: &str) -> Option<(String, &'static str)> {
    let tail = rel.strip_prefix("./").unwrap_or(rel);
    let mut tried: Vec<(String, &'static str)> = roots
        .iter()
        .filter(|root| matches!(root.via, RootVia::Document | RootVia::PaneCwd | RootVia::SessionCwd | RootVia::GitToplevel))
        .map(|root| (join(&root.dir, tail), source_of(&root.via)))
        .collect();
    for candidate in crawl_candidates(rel, cwd, repo_root, home, MAX_RUNGS) {
        if !tried.iter().any(|(seen, _)| seen == &candidate.0) {
            tried.push(candidate);
        }
    }
    tried.into_iter().find(|(path, _)| std::fs::symlink_metadata(path).is_ok())
}

/// The token under every other worktree of the pane's repositories. One hit
/// opens; several are a choice between branches.
fn worktree_join(rel: &str, roots: &[Root], line: Option<u32>) -> Option<ResolveResult> {
    let tail = rel.strip_prefix("./").unwrap_or(rel);
    let mut paths = Vec::new();
    let mut worktrees = Vec::new();
    for root in roots {
        let RootVia::Worktree(name) = &root.via else { continue };
        let candidate = join(&root.dir, tail);
        if std::fs::symlink_metadata(&candidate).is_ok() {
            paths.push(candidate);
            worktrees.push(name.clone());
        }
    }
    match paths.len() {
        0 => None,
        1 => Some(ResolveResult::Hit {
            reference: ResolvedRef { path: paths.remove(0), line, source: "worktree" },
        }),
        _ => {
            paths.truncate(MAX_CHOICES);
            worktrees.truncate(MAX_CHOICES);
            Some(ResolveResult::Choices { paths, line, via: "worktree", worktrees })
        }
    }
}

/// A token resolved against the click's roots. `roots[0]` is the pane cwd.
pub fn resolve(token: &str, roots: &[Root], home: &str, evidence: &AgentEvidence) -> ResolveResult {
    let clean = token.trim().trim_matches(|c| c == '\'' || c == '"' || c == '`');
    if clean.is_empty() {
        return ResolveResult::Miss;
    }
    let (rel, line) = split_line_ref(clean);
    if rel.is_empty() {
        return ResolveResult::Miss;
    }

    // An absolute token names exactly one path, so the ladder answers it here:
    // the rungs below join RELATIVE tokens onto directories, and answering a
    // stitched absolute token (several paths a soft join chained) with some
    // other file that shares its tail would open the wrong file. Nothing on
    // disk means a miss, and the click falls back to its row-scoped candidate.
    if rel.starts_with('/') || rel.starts_with("~/") {
        if !absolute_on_disk(&rel, home) {
            return ResolveResult::Miss;
        }
        return ResolveResult::Hit {
            reference: ResolvedRef { path: rel, line, source: "absolute" },
        };
    }

    if let Some(found) = resolve_from_evidence(&rel, evidence) {
        return match found {
            ResolveResult::Hit { reference } => ResolveResult::Hit {
                reference: ResolvedRef { line, ..reference },
            },
            ResolveResult::Choices { paths, via, worktrees, .. } => ResolveResult::Choices { paths, line, via, worktrees },
            other => other,
        };
    }

    let cwd = roots.first().map(|root| root.dir.to_string_lossy().into_owned()).unwrap_or_default();
    let repo_root = repo_root_for(&cwd);
    let search_root = repo_root.clone().unwrap_or_else(|| cwd.clone());

    if !looks_like_path(&rel) {
        if search_root.is_empty() {
            return ResolveResult::Miss;
        }
        let entries = index_for(Path::new(&search_root));
        return match unique_dir_named(&rel, &entries) {
            Some(path) => ResolveResult::Hit {
                reference: ResolvedRef { path, line: None, source: "fuzzy" },
            },
            None => ResolveResult::Miss,
        };
    }

    if let Some((path, source)) = local_join(&rel, roots, &cwd, repo_root.as_deref(), home) {
        return ResolveResult::Hit { reference: ResolvedRef { path, line, source } };
    }
    if let Some(found) = worktree_join(&rel, roots, line) {
        return found;
    }
    let tail = rel.strip_prefix("./").unwrap_or(&rel);
    for root in roots.iter().filter(|root| root.via == RootVia::Touched) {
        let candidate = join(&root.dir, tail);
        if std::fs::symlink_metadata(&candidate).is_ok() {
            return ResolveResult::Hit { reference: ResolvedRef { path: candidate, line, source: "touched" } };
        }
    }

    if search_root.is_empty() {
        return ResolveResult::Miss;
    }
    let entries = index_for(Path::new(&search_root));
    let exact = rank_exact(&rel, &entries);
    // One whole-tail match is unambiguous even when the bare filename repeats.
    let tails: Vec<&(String, bool)> = exact.iter().filter(|(_, tail)| *tail).collect();
    if exact.len() == 1 || tails.len() == 1 {
        let path = tails.first().map_or_else(|| exact[0].0.clone(), |(path, _)| path.clone());
        return ResolveResult::Hit {
            reference: ResolvedRef { path, line, source: "search" },
        };
    }
    if exact.len() > 1 {
        let mut paths: Vec<String> = exact.into_iter().map(|(path, _)| path).collect();
        paths.truncate(MAX_CHOICES);
        return ResolveResult::Choices { paths, line, via: "exact", worktrees: Vec::new() };
    }

    // A file inside a gitignored directory: `out/timeline.txt` under a lab whose
    // .gitignore hides `out`. The walker never lists the file, but it lists the
    // lab, so the token joined to every indexed directory finds it with a stat.
    let ignored = under_indexed_dirs(&rel, &entries);
    if ignored.len() == 1 {
        return ResolveResult::Hit {
            reference: ResolvedRef { path: ignored[0].clone(), line, source: "ignored" },
        };
    }
    if ignored.len() > 1 {
        let mut paths = ignored;
        paths.truncate(MAX_CHOICES);
        return ResolveResult::Choices { paths, line, via: "exact", worktrees: Vec::new() };
    }

    for candidate in sibling_candidates(&rel, &cwd, repo_root.as_deref(), home, MAX_RUNGS) {
        if std::fs::symlink_metadata(&candidate).is_ok() {
            return ResolveResult::Hit {
                reference: ResolvedRef { path: candidate, line, source: "sibling" },
            };
        }
    }

    let fuzzy = rank_fuzzy(&rel, &entries, 20);
    if fuzzy.is_empty() {
        // Nothing on disk anywhere. A path-shaped token may still be a file git
        // holds and the working tree does not.
        if rel.contains('/') {
            let repos = git_probe_repos(&rel, &cwd, repo_root.as_deref(), home);
            if let Some((repo, rev, subject)) = git_absent(&rel, &repos) {
                return ResolveResult::Absent { repo, rev, path: rel, subject };
            }
        }
        return ResolveResult::Miss;
    }
    ResolveResult::Choices {
        paths: fuzzy.into_iter().map(|(path, _)| path).collect(),
        line,
        via: "fuzzy",
        worktrees: Vec::new(),
    }
}

/// A token written in the markdown document at `doc`. The document's own
/// roots answer first: a join onto its directory, then its checkout, then the
/// token under that repository's worktrees. Anything else runs the ladder over
/// the document's roots followed by `roots`, so the index search walks the
/// document's checkout.
pub fn resolve_in_doc(token: &str, doc: &Path, roots: &[Root], home: &str, evidence: &AgentEvidence) -> ResolveResult {
    let own = doc_roots(doc, &[]);
    let clean = token.trim().trim_matches(|c| c == '\'' || c == '"' || c == '`');
    let (rel, line) = split_line_ref(clean);
    let relative = !rel.is_empty() && !rel.starts_with('/') && !rel.starts_with("~/");
    if relative && looks_like_path(&rel) {
        let tail = rel.strip_prefix("./").unwrap_or(&rel);
        for root in own.iter().filter(|root| matches!(root.via, RootVia::Document | RootVia::GitToplevel)) {
            let candidate = join(&root.dir, tail);
            if std::fs::symlink_metadata(&candidate).is_ok() {
                return ResolveResult::Hit { reference: ResolvedRef { path: candidate, line, source: source_of(&root.via) } };
            }
        }
        if let Some(found) = worktree_join(&rel, &own, line) {
            return found;
        }
    }
    resolve(token, &doc_roots(doc, roots), home, evidence)
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

    /// RECEIPT. A repo-relative token whose file sits inside a gitignored
    /// directory resolves to that file, before fzf gets to guess.
    #[test]
    fn a_file_inside_a_gitignored_directory_still_resolves() {
        let root = std::env::temp_dir().join(format!("instant-ignored-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lab = root.join("labs").join("otel");
        std::fs::create_dir_all(lab.join("out")).unwrap();
        std::fs::write(lab.join(".gitignore"), "out\n").unwrap();
        std::fs::write(lab.join("out").join("timeline.txt"), "0.371s span>\n").unwrap();
        std::fs::write(root.join("README.md"), "root\n").unwrap();
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["init", "-q"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "git init in {}", root.display());
        clear_index_cache();
        let entries = index_for(&root);
        assert!(
            !entries.iter().any(|e| e.path.ends_with("out/timeline.txt")),
            "the walker honours .gitignore, so the file is not indexed"
        );
        let cwd = root.to_string_lossy().into_owned();
        let result = resolve_with("out/timeline.txt", &cwd, &cwd, &AgentEvidence::default());
        let expected = lab.join("out").join("timeline.txt").to_string_lossy().into_owned();
        assert_eq!(
            result,
            ResolveResult::Hit {
                reference: ResolvedRef { path: expected, line: None, source: "ignored" }
            }
        );
        let _ = std::fs::remove_dir_all(&root);
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

    struct Tree(PathBuf);

    impl Tree {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("instant-refresolve-{name}"));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Tree(root)
        }

        fn file(&self, rel: &str) -> PathBuf {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "x").unwrap();
            path
        }

        fn path(&self, rel: &str) -> String {
            self.0.join(rel).to_string_lossy().into_owned()
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn resolve(tree: &Tree, token: &str, cwd: &str) -> ResolveResult {
        clear_index_cache();
        resolve_with(token, cwd, &tree.0.to_string_lossy(), &AgentEvidence::default())
    }

    #[test]
    fn resolves_against_a_real_tree() {
        let tree = Tree::new("ladder");
        tree.file("projects/instant/.git/HEAD");
        tree.file("projects/instant/src/main.ts");
        tree.file("projects/instant/src/preview.ts");
        tree.file("projects/instant/e2e/MdPanel.tsx");
        tree.file("projects/instant/src/mdview/MdPanel.tsx");
        tree.file("projects/instant-lanes/README.md");
        tree.file("TODO.md");
        let cwd = tree.path("projects/instant/src");

        assert_eq!(
            resolve(&tree, "main.ts", &cwd),
            ResolveResult::Hit {
                reference: ResolvedRef {
                    path: tree.path("projects/instant/src/main.ts"),
                    line: None,
                    source: "cwd",
                },
            }
        );
        assert_eq!(
            resolve(&tree, "main.ts:214", &cwd),
            ResolveResult::Hit {
                reference: ResolvedRef {
                    path: tree.path("projects/instant/src/main.ts"),
                    line: Some(214),
                    source: "cwd",
                },
            }
        );
        assert_eq!(
            resolve(&tree, "e2e/MdPanel.tsx", &cwd),
            ResolveResult::Hit {
                reference: ResolvedRef {
                    path: tree.path("projects/instant/e2e/MdPanel.tsx"),
                    line: None,
                    source: "repo",
                },
            }
        );
        assert_eq!(
            resolve(&tree, "instant-lanes/README.md", &cwd),
            ResolveResult::Hit {
                reference: ResolvedRef {
                    path: tree.path("projects/instant-lanes/README.md"),
                    line: None,
                    source: "ancestor",
                },
            }
        );
        assert_eq!(
            resolve(&tree, "TODO.md", &cwd),
            ResolveResult::Hit {
                reference: ResolvedRef { path: tree.path("TODO.md"), line: None, source: "ancestor" },
            }
        );
    }

    #[test]
    fn falls_through_exact_then_fuzzy_then_ripgrep() {
        let tree = Tree::new("fallthrough");
        tree.file("repo/.git/HEAD");
        tree.file("repo/src/preview.ts");
        tree.file("repo/src/mdview/MdPanel.tsx");
        tree.file("repo/e2e/MdPanel.tsx");
        tree.file("repo/packages/patchset-diff/src/index.ts");
        let cwd = tree.path("repo/src");

        assert_eq!(
            resolve(&tree, "MdPanel.tsx", &cwd),
            ResolveResult::Choices {
                paths: vec![
                    tree.path("repo/e2e/MdPanel.tsx"),
                    tree.path("repo/src/mdview/MdPanel.tsx"),
                ],
                line: None,
                via: "exact",
                worktrees: Vec::new(),
            }
        );
        assert_eq!(
            resolve(&tree, "prevew.ts", &cwd),
            ResolveResult::Choices {
                paths: vec![tree.path("repo/src/preview.ts")],
                line: None,
                via: "fuzzy",
                worktrees: Vec::new(),
            }
        );
        assert_eq!(
            resolve(&tree, "patchset-diff", &cwd),
            ResolveResult::Hit {
                reference: ResolvedRef {
                    path: tree.path("repo/packages/patchset-diff"),
                    line: None,
                    source: "fuzzy",
                },
            }
        );
        assert_eq!(resolve(&tree, "qqqzzz.ts", &cwd), ResolveResult::Miss);
        assert_eq!(resolve(&tree, "renderPathInto", &cwd), ResolveResult::Miss);
    }

    fn git(tree: &Tree, rel: &str, args: &[&str]) -> String {
        let repo = tree.0.join(rel);
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn crawls_sideways_into_a_sibling_checkout() {
        let tree = Tree::new("siblings");
        tree.file("projects/instant/.git/HEAD");
        tree.file("projects/instant/src/main.ts");
        tree.file("projects/sprefa/.git/HEAD");
        tree.file("projects/sprefa/plans/bench/STUDY.md");
        let cwd = tree.path("projects/instant/src");

        // The token is relative to sprefa's root, so no ancestor join reaches it.
        let joins = crawl_candidates("plans/bench/STUDY.md", &cwd, None, &tree.0.to_string_lossy(), 8);
        let _ = sibling_candidates("plans/bench/STUDY.md", &cwd, None, &tree.0.to_string_lossy(), 8);
        assert!(joins.iter().all(|(path, _)| std::fs::symlink_metadata(path).is_err()));

        assert_eq!(
            resolve(&tree, "plans/bench/STUDY.md", &cwd),
            ResolveResult::Hit {
                reference: ResolvedRef {
                    path: tree.path("projects/sprefa/plans/bench/STUDY.md"),
                    line: None,
                    source: "sibling",
                },
            }
        );
    }

    #[test]
    fn reports_a_path_only_git_still_holds() {
        let tree = Tree::new("gitabsent");
        tree.file("projects/instant/.git/HEAD");
        tree.file("projects/sprefa/plans/keep.md");
        tree.file("projects/sprefa/plans/STUDY.md");
        git(&tree, "projects/sprefa", &["init", "-q", "-b", "main"]);
        git(&tree, "projects/sprefa", &["add", "-A"]);
        git(&tree, "projects/sprefa", &["commit", "-qm", "study doc"]);
        let rev = git(&tree, "projects/sprefa", &["rev-parse", "HEAD"]);
        std::fs::remove_file(tree.0.join("projects/sprefa/plans/STUDY.md")).unwrap();
        git(&tree, "projects/sprefa", &["commit", "-aqm", "drop it"]);
        let cwd = tree.path("projects/instant");

        match resolve(&tree, "plans/STUDY.md", &cwd) {
            ResolveResult::Absent { repo, rev: found, path, subject } => {
                assert_eq!(repo, tree.path("projects/sprefa"));
                assert_eq!(found, rev);
                assert_eq!(path, "plans/STUDY.md");
                assert!(subject.ends_with("study doc"), "{subject}");
            }
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn a_stitched_absolute_path_is_not_a_hit() {
        let tree = Tree::new("softjoin");
        tree.file("playwright.readme.config.ts");
        tree.file("docs/screenshots/07-turn-strip.png");
        tree.file("docs/screenshots/08-turn-strip-popover.png");
        let cwd = tree.path("docs/screenshots");
        // The shape a soft join builds from a block of one-path-per-line rows:
        // the second path's basename matches a file on disk, the concatenation
        // matches nothing.
        let first = tree.path("playwright.readme.config.ts");
        let second = tree.path("docs/screenshots/07-turn-strip.png");
        let stitched = format!("{first}{second}");

        assert_eq!(resolve(&tree, &stitched, &cwd), ResolveResult::Miss);
        assert_eq!(
            resolve(&tree, &second, &cwd),
            ResolveResult::Hit {
                reference: ResolvedRef { path: second, line: None, source: "absolute" },
            }
        );
    }

    #[test]
    fn serializes_the_shape_the_renderer_expects() {
        let hit = ResolveResult::Hit {
            reference: ResolvedRef { path: "/a/b.ts".into(), line: Some(9), source: "cwd" },
        };
        assert_eq!(
            serde_json::to_string(&hit).unwrap(),
            r#"{"kind":"hit","ref":{"path":"/a/b.ts","line":9,"source":"cwd"}}"#
        );
        let choices =
            ResolveResult::Choices { paths: vec!["/a/b.ts".into()], line: None, via: "fuzzy", worktrees: Vec::new() };
        assert_eq!(
            serde_json::to_string(&choices).unwrap(),
            r#"{"kind":"choices","paths":["/a/b.ts"],"via":"fuzzy"}"#
        );
        assert_eq!(serde_json::to_string(&ResolveResult::Miss).unwrap(), r#"{"kind":"miss"}"#);
        let absent = ResolveResult::Absent {
            repo: "/r".into(),
            rev: "abc".into(),
            path: "plans/x.md".into(),
            subject: "abc study".into(),
        };
        assert_eq!(
            serde_json::to_string(&absent).unwrap(),
            r#"{"kind":"absent","repo":"/r","rev":"abc","path":"plans/x.md","subject":"abc study"}"#
        );
    }
}