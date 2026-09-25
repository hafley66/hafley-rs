//! The filesystem ⌘-click ladder over click roots: absolute, joins onto each
//! root, worktree fan-out, the index, siblings, fzf, then git history. A caller
//! holding agent evidence runs its own rung in front of `resolve_fs`.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::_1_pane_at::PaneHit;
use crate::_2_click_rungs::*;
use crate::_3_click_roots::{click_roots, doc_roots, repos_beside, worktrees_of, Root, RootVia};

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
        /// Parallel to `paths` when `via` is `worktree` (the branch each path is
        /// on) or `sibling` (the repository directory each path is in).
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

/// What the filesystem ladder answers for one click: the roots it searched
/// and the result.
#[derive(Debug, PartialEq, Eq)]
pub struct FsLookup {
    pub roots: Vec<Root>,
    pub result: ResolveResult,
}

/// ⌘-click on `token` in the pane `hit` names, filesystem only. A token
/// written in a markdown document (`doc`) resolves from that document's roots
/// first. `home` bounds the ancestor walk.
pub fn cmd_click_lookup(hit: &PaneHit, token: &str, doc: Option<&Path>, home: &str) -> FsLookup {
    let roots = click_roots(&hit.pane_current_path, &[], &[]);
    match doc {
        Some(doc) => FsLookup { result: resolve_fs_in_doc(token, doc, &roots, home), roots: doc_roots(doc, &roots) },
        None => FsLookup { result: resolve_fs(token, &roots, home), roots },
    }
}

/// A clicked token with its quotes stripped and its line reference split off.
/// `None` for a token that is empty either way.
pub fn clean_token(token: &str) -> Option<(String, Option<u32>)> {
    let clean = token.trim().trim_matches(|c| c == '\'' || c == '"' || c == '`');
    if clean.is_empty() {
        return None;
    }
    let (rel, line) = split_line_ref(clean);
    if rel.is_empty() {
        return None;
    }
    Some((rel, line))
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

/// The token under every git repository beside the pane's checkout (the
/// checkout's parent's other children). One hit opens; several are a choice
/// tagged by repository name.
fn sibling_join(rel: &str, toplevel: Option<&str>, line: Option<u32>) -> Option<ResolveResult> {
    let tail = rel.strip_prefix("./").unwrap_or(rel);
    let mut paths = Vec::new();
    let mut repos = Vec::new();
    for repo in repos_beside(Path::new(toplevel?)) {
        let candidate = join(&repo, tail);
        if std::fs::symlink_metadata(&candidate).is_ok() {
            paths.push(candidate);
            repos.push(repo.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default());
        }
    }
    match paths.len() {
        0 => None,
        1 => Some(ResolveResult::Hit {
            reference: ResolvedRef { path: paths.remove(0), line, source: "sibling" },
        }),
        _ => {
            paths.truncate(MAX_CHOICES);
            repos.truncate(MAX_CHOICES);
            Some(ResolveResult::Choices { paths, line, via: "sibling", worktrees: repos })
        }
    }
}

/// A checkout the exact rung ranks, the label its rows carry in a choice, and
/// the source a single hit in it answers with.
struct ExactGroup {
    dir: PathBuf,
    label: String,
    source: &'static str,
}

/// The checkouts the exact rung ranks, in answer order: the pane's own
/// (`search_root`) first, then every other worktree among `roots`, then the
/// repositories beside the trunk. The repository's main worktree is `trunk`.
fn exact_groups(search_root: &str, roots: &[Root]) -> Vec<ExactGroup> {
    let canonical = |dir: &Path| std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let own = PathBuf::from(search_root);
    let worktrees = worktrees_of(&own);
    let trunk = worktrees.first().map(|(dir, _)| canonical(dir));
    let label = |dir: &Path, fallback: &str| {
        let dir = canonical(dir);
        if trunk.as_ref() == Some(&dir) {
            return "trunk".to_owned();
        }
        match worktrees.iter().find(|(wt, _)| canonical(wt) == dir) {
            Some((_, name)) => format!("worktree {name}"),
            None => fallback.to_owned(),
        }
    };
    let mut groups = vec![ExactGroup { label: label(&own, "cwd"), dir: own.clone(), source: "search" }];
    for root in roots {
        let RootVia::Worktree(name) = &root.via else { continue };
        groups.push(ExactGroup { label: label(&root.dir, &format!("worktree {name}")), dir: root.dir.clone(), source: "worktree" });
    }
    let mut seen: Vec<PathBuf> = groups.iter().map(|group| canonical(&group.dir)).collect();
    seen.extend(worktrees.iter().map(|(dir, _)| canonical(dir)));
    let beside = worktrees.first().map_or(own, |(dir, _)| dir.clone());
    for repo in repos_beside(&beside) {
        if seen.contains(&canonical(&repo)) {
            continue;
        }
        let name = repo.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        groups.push(ExactGroup { label: format!("sibling {name}"), dir: repo, source: "sibling" });
    }
    groups
}

struct ExactMatch {
    path: String,
    tail: bool,
    group: usize,
}

/// `rank_exact` over the index of each group in `ranked`. Group 0 (the pane's
/// checkout) keeps name-only matches; the rest keep whole-tail matches. A path
/// belongs to the deepest group containing it. A worktree row at the same
/// checkout-relative path as a group-0 row is that file on another branch and
/// drops out.
fn exact_matches(rel: &str, groups: &[ExactGroup], ranked: std::ops::Range<usize>) -> Vec<ExactMatch> {
    let under = |path: &str, dir: &Path| path.strip_prefix(&format!("{}/", trim_slash(&dir.to_string_lossy()))).map(str::to_owned);
    let mut found: Vec<ExactMatch> = Vec::new();
    for index in ranked {
        for (path, tail) in rank_exact(rel, &index_for(&groups[index].dir)) {
            if (index > 0 && !tail) || found.iter().any(|seen| seen.path == path) {
                continue;
            }
            let owner = groups
                .iter()
                .enumerate()
                .filter(|(_, group)| under(&path, &group.dir).is_some())
                .max_by_key(|(_, group)| group.dir.as_os_str().len())
                .map_or(index, |(owner, _)| owner);
            found.push(ExactMatch { path, tail, group: owner });
        }
    }
    let first: Vec<String> = found
        .iter()
        .filter(|found| found.group == 0)
        .filter_map(|found| under(&found.path, &groups[0].dir))
        .collect();
    found.retain(|found| {
        found.group == 0
            || groups[found.group].source != "worktree"
            || under(&found.path, &groups[found.group].dir).is_none_or(|rel| !first.contains(&rel))
    });
    found.sort_by_key(|found| found.group);
    found
}

/// One match, or one whole-tail match among several, opens; several are a
/// choice, tagged by checkout when they span more than one.
fn answer_exact(found: Vec<ExactMatch>, groups: &[ExactGroup], line: Option<u32>) -> Option<ResolveResult> {
    let tails: Vec<&ExactMatch> = found.iter().filter(|found| found.tail).collect();
    if found.len() == 1 || tails.len() == 1 {
        let only = tails.first().copied().unwrap_or(&found[0]);
        return Some(ResolveResult::Hit {
            reference: ResolvedRef { path: only.path.clone(), line, source: groups[only.group].source },
        });
    }
    if found.is_empty() {
        return None;
    }
    let mut labels: Vec<String> = found.iter().map(|found| groups[found.group].label.clone()).collect();
    let spans = labels.iter().any(|label| label != &labels[0]);
    let mut paths: Vec<String> = found.into_iter().map(|found| found.path).collect();
    paths.truncate(MAX_CHOICES);
    labels.truncate(MAX_CHOICES);
    Some(ResolveResult::Choices { paths, line, via: "exact", worktrees: if spans { labels } else { Vec::new() } })
}

/// A token resolved against the click's roots. `roots[0]` is the pane cwd.
pub fn resolve_fs(token: &str, roots: &[Root], home: &str) -> ResolveResult {
    let Some((rel, line)) = clean_token(token) else {
        return ResolveResult::Miss;
    };

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
    if let Some(found) = sibling_join(&rel, repo_root.as_deref(), line) {
        return found;
    }

    if search_root.is_empty() {
        return ResolveResult::Miss;
    }
    let entries = index_for(Path::new(&search_root));
    // A bare filename is a choice between every checkout of the pane's
    // repository that holds one: its own, then the other worktrees (untracked
    // files in an ignored `.boop-worktrees` lane included). A token with a
    // directory in it stays with the pane's checkout while any rung there
    // answers. Sibling repositories rank only a token with a directory in it.
    let groups = exact_groups(&search_root, roots);
    let checkouts = groups.iter().filter(|group| group.source != "sibling").count();
    let bare = !tail.trim_end_matches('/').contains('/');
    let ranked = if bare { 0..checkouts } else { 0..1 };
    if let Some(found) = answer_exact(exact_matches(&rel, &groups, ranked), &groups, line) {
        return found;
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

    // The tail under another worktree or a sibling repository's subdirectory:
    // `plugins/files/1_FileTree.tsx` printed from `<sibling>/src`.
    if !bare {
        if let Some(found) = answer_exact(exact_matches(&rel, &groups, 1..groups.len()), &groups, line) {
            return found;
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

/// The rung a token written in the markdown document at `doc` answers first:
/// a join onto the document's directory, then its checkout, then the token
/// under that repository's worktrees.
pub fn doc_join(token: &str, doc: &Path) -> Option<ResolveResult> {
    let (rel, line) = clean_token(token)?;
    if rel.starts_with('/') || rel.starts_with("~/") || !looks_like_path(&rel) {
        return None;
    }
    let own = doc_roots(doc, &[]);
    let tail = rel.strip_prefix("./").unwrap_or(&rel);
    for root in own.iter().filter(|root| matches!(root.via, RootVia::Document | RootVia::GitToplevel)) {
        let candidate = join(&root.dir, tail);
        if std::fs::symlink_metadata(&candidate).is_ok() {
            return Some(ResolveResult::Hit { reference: ResolvedRef { path: candidate, line, source: source_of(&root.via) } });
        }
    }
    worktree_join(&rel, &own, line)
}

/// A token written in the markdown document at `doc`: `doc_join`, else the
/// ladder over the document's roots followed by `roots`, so the index search
/// walks the document's checkout.
pub fn resolve_fs_in_doc(token: &str, doc: &Path, roots: &[Root], home: &str) -> ResolveResult {
    doc_join(token, doc).unwrap_or_else(|| resolve_fs(token, &doc_roots(doc, roots), home))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(cwd: &str) -> PaneHit {
        PaneHit { pane: String::new(), pane_current_path: PathBuf::from(cwd), pane_col: 0, pane_row: 0 }
    }

    fn resolve_with(token: &str, cwd: &str, home: &str) -> ResolveResult {
        cmd_click_lookup(&pane(cwd), token, None, home).result
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
        let result = resolve_with("out/timeline.txt", &cwd, &cwd);
        let expected = lab.join("out").join("timeline.txt").to_string_lossy().into_owned();
        assert_eq!(
            result,
            ResolveResult::Hit {
                reference: ResolvedRef { path: expected, line: None, source: "ignored" }
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
        resolve_with(token, cwd, &tree.0.to_string_lossy())
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

    fn git_in(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} in {}", dir.display());
    }

    fn put(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x").unwrap();
    }

    /// `projects/trunk` (committed `site/index.html`, `.boop-worktrees/`
    /// ignored), its linked worktree `projects/trunk-feat` (untracked
    /// `docs/index.html`), a boop lane worktree
    /// `trunk/.boop-worktrees/chore/x` (untracked `plans/p/index.html`), and a
    /// sibling repo `projects/instant` holding `src/plugins/files/1_FileTree.tsx`.
    fn lane_tree() -> (tempfile::TempDir, PathBuf) {
        let scratch = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(scratch.path()).unwrap();
        let trunk = base.join("projects/trunk");
        put(&trunk.join("site/index.html"));
        std::fs::write(trunk.join(".gitignore"), ".boop-worktrees/\n").unwrap();
        git_in(&trunk, &["init", "-q", "-b", "main"]);
        git_in(&trunk, &["add", "-A"]);
        git_in(&trunk, &["commit", "-qm", "site"]);
        git_in(&trunk, &["worktree", "add", "-q", "-b", "feat", base.join("projects/trunk-feat").to_str().unwrap()]);
        put(&base.join("projects/trunk-feat/docs/index.html"));
        git_in(&trunk, &["worktree", "add", "-q", "-b", "chore/x", trunk.join(".boop-worktrees/chore/x").to_str().unwrap()]);
        put(&trunk.join(".boop-worktrees/chore/x/plans/p/index.html"));
        let sibling = base.join("projects/instant");
        put(&sibling.join("src/plugins/files/1_FileTree.tsx"));
        git_in(&sibling, &["init", "-q", "-b", "main"]);
        clear_index_cache();
        (scratch, base)
    }

    fn debug_under(base: &Path, value: &impl std::fmt::Debug) -> String {
        format!("{value:#?}").replace(&format!("{}/", base.display()), "")
    }

    /// RECEIPT (click-bare-name-choices). A bare filename lists every
    /// reachable match: the pane's checkout (trunk), its linked worktree, and
    /// the untracked file in a `.boop-worktrees` lane the trunk ignores. The
    /// tracked copies of `site/index.html` in the other worktrees are the
    /// trunk's file on another branch and stay out.
    #[test]
    fn a_bare_filename_lists_every_worktree_match_grouped_by_root() {
        let (_scratch, base) = lane_tree();
        let hit = pane(&base.join("projects/trunk").to_string_lossy());
        let lookup = cmd_click_lookup(&hit, "index.html", None, &base.to_string_lossy());
        assert_eq!(
            debug_under(&base, &lookup.result),
            r#"Choices {
    paths: [
        "projects/trunk/site/index.html",
        "projects/trunk-feat/docs/index.html",
        "projects/trunk/.boop-worktrees/chore/x/plans/p/index.html",
    ],
    line: None,
    via: "exact",
    worktrees: [
        "trunk",
        "worktree feat",
        "worktree chore/x",
    ],
}"#
        );
    }

    /// RECEIPT (click-bare-name-choices). A token relative to a subdirectory
    /// of a sibling repository resolves by exact tail over that sibling's
    /// index once every earlier rung misses.
    #[test]
    fn a_tail_under_a_sibling_repository_subdirectory_resolves_there() {
        let (_scratch, base) = lane_tree();
        let hit = pane(&base.join("projects/trunk").to_string_lossy());
        let lookup = cmd_click_lookup(&hit, "plugins/files/1_FileTree.tsx:12", None, &base.to_string_lossy());
        assert_eq!(
            debug_under(&base, &lookup.result),
            r#"Hit {
    reference: ResolvedRef {
        path: "projects/instant/src/plugins/files/1_FileTree.tsx",
        line: Some(
            12,
        ),
        source: "sibling",
    },
}"#
        );
    }

    /// RECEIPT. boop-mux answers a click from the pane alone, with no agent
    /// store: from the linked worktree the roots fan out to the trunk and the
    /// lane, and the pane's own checkout groups first.
    #[test]
    fn the_lookup_from_a_linked_worktree_needs_only_the_filesystem() {
        let (_scratch, base) = lane_tree();
        let hit = pane(&base.join("projects/trunk-feat").to_string_lossy());
        let lookup = cmd_click_lookup(&hit, "index.html", None, &base.to_string_lossy());
        assert_eq!(
            debug_under(&base, &lookup),
            r#"FsLookup {
    roots: [
        Root {
            dir: "projects/trunk-feat",
            via: PaneCwd,
        },
        Root {
            dir: "projects/trunk",
            via: Worktree(
                "main",
            ),
        },
        Root {
            dir: "projects/trunk/.boop-worktrees/chore/x",
            via: Worktree(
                "chore/x",
            ),
        },
    ],
    result: Choices {
        paths: [
            "projects/trunk-feat/docs/index.html",
            "projects/trunk-feat/site/index.html",
            "projects/trunk/.boop-worktrees/chore/x/plans/p/index.html",
        ],
        line: None,
        via: "exact",
        worktrees: [
            "worktree feat",
            "worktree feat",
            "worktree chore/x",
        ],
    },
}"#
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