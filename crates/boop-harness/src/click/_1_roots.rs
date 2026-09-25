//! The directories a clicked token may be relative to, nearest first: pane cwd,
//! session cwds, their checkouts, every worktree of those, touched checkouts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use boop_mux::PaneHit;
use boop_store::SessionTouched;

use super::_0_rungs::{repo_root_for, MAX_SIBLINGS};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootVia {
    /// The directory of the document the token was written in.
    Document,
    PaneCwd,
    SessionCwd,
    GitToplevel,
    /// A worktree of a toplevel above, named by its branch (or directory
    /// name when detached).
    Worktree(String),
    Touched,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root {
    pub dir: PathBuf,
    pub via: RootVia,
}

/// The ordered, canonically deduped roots for a click in `pane`.
pub fn click_roots(pane: &PaneHit, touched: &SessionTouched) -> Vec<Root> {
    let mut roots = Roots::default();
    roots.push(pane.pane_current_path.clone(), RootVia::PaneCwd);
    for cwd in &touched.cwds {
        roots.push(PathBuf::from(cwd), RootVia::SessionCwd);
    }
    let local: Vec<PathBuf> = roots.list.iter().map(|root| root.dir.clone()).collect();
    let mut toplevels: Vec<PathBuf> = Vec::new();
    for dir in &local {
        if let Some(top) = repo_root_for(&dir.to_string_lossy()).map(PathBuf::from) {
            if !toplevels.contains(&top) {
                toplevels.push(top);
            }
        }
    }
    for top in &toplevels {
        roots.push(top.clone(), RootVia::GitToplevel);
    }
    for top in &toplevels {
        for (dir, name) in worktrees_of(top) {
            roots.push(dir, RootVia::Worktree(name));
        }
    }
    for path in &touched.paths {
        let parent = Path::new(path).parent().map(|dir| dir.to_string_lossy().into_owned());
        let Some(parent) = parent else { continue };
        let dir = repo_root_for(&parent).unwrap_or(parent);
        roots.push(PathBuf::from(dir), RootVia::Touched);
    }
    roots.list
}

/// The roots of a token written in the document at `doc`, nearest first: the
/// document's directory, its checkout, that repository's worktrees. `rest`
/// (the click's own roots) follows, deduped against them.
pub fn doc_roots(doc: &Path, rest: &[Root]) -> Vec<Root> {
    let mut roots = Roots::default();
    let dir = if doc.is_dir() { Some(doc) } else { doc.parent() };
    if let Some(dir) = dir {
        roots.push(dir.to_path_buf(), RootVia::Document);
        if let Some(top) = repo_root_for(&dir.to_string_lossy()).map(PathBuf::from) {
            roots.push(top.clone(), RootVia::GitToplevel);
            for (worktree, name) in worktrees_of(&top) {
                roots.push(worktree, RootVia::Worktree(name));
            }
        }
    }
    for root in rest {
        roots.push(root.dir.clone(), root.via.clone());
    }
    roots.list
}

#[derive(Default)]
struct Roots {
    list: Vec<Root>,
    seen: Vec<PathBuf>,
}

impl Roots {
    fn push(&mut self, dir: PathBuf, via: RootVia) {
        if dir.as_os_str().is_empty() {
            return;
        }
        let canonical = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        if self.seen.contains(&canonical) {
            return;
        }
        self.seen.push(canonical);
        self.list.push(Root { dir, via });
    }
}

/// The directory holding `worktrees/`: `.git` itself for a main checkout, or
/// the `commondir` a linked worktree's gitdir points back to.
fn common_git_dir(toplevel: &Path) -> Option<PathBuf> {
    let dot_git = toplevel.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let gitdir = toplevel.join(text.trim().strip_prefix("gitdir:")?.trim());
    let common = std::fs::read_to_string(gitdir.join("commondir")).ok()?;
    Some(gitdir.join(common.trim()))
}

type WorktreeCache = Mutex<HashMap<PathBuf, (Option<SystemTime>, Vec<(PathBuf, String)>)>>;

fn worktree_cache() -> &'static WorktreeCache {
    static CACHE: OnceLock<WorktreeCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Every worktree of the repository `toplevel` belongs to, main checkout
/// included. Cached per common git dir until `worktrees/` changes mtime.
pub fn worktrees_of(toplevel: &Path) -> Vec<(PathBuf, String)> {
    let Some(common) = common_git_dir(toplevel) else {
        return Vec::new();
    };
    let common = std::fs::canonicalize(&common).unwrap_or(common);
    let stamp = std::fs::metadata(common.join("worktrees")).and_then(|meta| meta.modified()).ok();
    if let Some((at, list)) = worktree_cache().lock().ok().and_then(|cache| cache.get(&common).cloned()) {
        if at == stamp {
            return list;
        }
    }
    let list = std::process::Command::new("git")
        .arg("-C")
        .arg(toplevel)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| parse_worktree_list(&String::from_utf8_lossy(&output.stdout)))
        .unwrap_or_default();
    if let Ok(mut cache) = worktree_cache().lock() {
        cache.insert(common, (stamp, list.clone()));
    }
    list
}

type BesideCache = Mutex<HashMap<PathBuf, (Option<SystemTime>, Vec<PathBuf>)>>;

fn beside_cache() -> &'static BesideCache {
    static CACHE: OnceLock<BesideCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The git repositories beside `toplevel`: immediate children of its parent
/// that hold a `.git` (the first MAX_SIBLINGS entries read), by name,
/// `toplevel` excluded. Cached per parent until the parent's mtime changes.
pub fn repos_beside(toplevel: &Path) -> Vec<PathBuf> {
    let Some(parent) = toplevel.parent() else {
        return Vec::new();
    };
    let stamp = std::fs::metadata(parent).and_then(|meta| meta.modified()).ok();
    let cached = beside_cache().lock().ok().and_then(|cache| cache.get(parent).cloned());
    let list = match cached {
        Some((at, list)) if at == stamp => list,
        _ => {
            let mut list: Vec<PathBuf> = std::fs::read_dir(parent)
                .map(|children| {
                    children
                        .flatten()
                        .take(MAX_SIBLINGS)
                        .filter(|child| !child.file_name().to_string_lossy().starts_with('.'))
                        .map(|child| child.path())
                        .filter(|path| path.is_dir() && path.join(".git").exists())
                        .collect()
                })
                .unwrap_or_default();
            list.sort();
            if let Ok(mut cache) = beside_cache().lock() {
                cache.insert(parent.to_path_buf(), (stamp, list.clone()));
            }
            list
        }
    };
    list.into_iter().filter(|repo| repo != toplevel).collect()
}

/// `git worktree list --porcelain`: blank-line separated records, `worktree
/// <path>` first, then `branch refs/heads/<name>` or `detached`. Bare repos skip.
pub fn parse_worktree_list(text: &str) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for record in text.split("\n\n") {
        let mut path = None;
        let mut branch = None;
        let mut bare = false;
        for line in record.lines() {
            if let Some(rest) = line.strip_prefix("worktree ") {
                path = Some(PathBuf::from(rest));
            } else if let Some(rest) = line.strip_prefix("branch ") {
                branch = Some(rest.strip_prefix("refs/heads/").unwrap_or(rest).to_owned());
            } else if line == "bare" {
                bare = true;
            }
        }
        let Some(path) = path else { continue };
        if bare || !path.is_dir() {
            continue;
        }
        let name = branch.unwrap_or_else(|| {
            path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
        });
        out.push((path, name));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn git(dir: &Path, args: &[&str]) {
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

    fn rel(roots: &[Root], base: &Path) -> Vec<(String, RootVia)> {
        roots
            .iter()
            .map(|root| (root.dir.strip_prefix(base).unwrap_or(&root.dir).display().to_string(), root.via.clone()))
            .collect()
    }

    #[test]
    fn roots_order_dedupe_and_fan_out_to_worktrees() {
        let scratch = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(scratch.path()).unwrap();
        let repo = base.join("repo");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/a.rs"), "a").unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "a"]);
        git(&repo, &["worktree", "add", "-q", "-b", "feat", base.join("repo-feat").to_str().unwrap()]);
        std::fs::create_dir_all(base.join("other/.git")).unwrap();
        std::fs::create_dir_all(base.join("other/lib")).unwrap();

        let pane = PaneHit {
            pane: "%1".into(),
            pane_current_path: repo.join("src"),
            pane_col: 0,
            pane_row: 0,
        };
        let touched = SessionTouched {
            paths: vec![
                base.join("other/lib/x.rs").display().to_string(),
                repo.join("src/a.rs").display().to_string(),
            ],
            // `repo/src/` is the pane cwd again, spelled with a trailing slash.
            cwds: vec![format!("{}/", repo.join("src").display()), repo.display().to_string()],
        };
        let roots = click_roots(&pane, &touched);
        assert_eq!(
            rel(&roots, &base),
            vec![
                ("repo/src".into(), RootVia::PaneCwd),
                ("repo".into(), RootVia::SessionCwd),
                ("repo-feat".into(), RootVia::Worktree("feat".into())),
                ("other".into(), RootVia::Touched),
            ]
        );

        git(&repo, &["worktree", "add", "-q", "--detach", base.join("repo-detached").to_str().unwrap()]);
        let names: Vec<RootVia> = worktrees_of(&repo).into_iter().map(|(_, name)| RootVia::Worktree(name)).collect();
        assert_eq!(
            names,
            vec![
                RootVia::Worktree("main".into()),
                RootVia::Worktree("repo-detached".into()),
                RootVia::Worktree("feat".into()),
            ],
            "a new worktree changes worktrees/ mtime, so the cache refreshes"
        );
        let from_linked: Vec<PathBuf> = worktrees_of(&base.join("repo-feat")).into_iter().map(|(dir, _)| dir).collect();
        assert_eq!(from_linked, vec![repo.clone(), base.join("repo-detached"), base.join("repo-feat")]);
    }

    /// RECEIPT. A markdown file anchors its refs: its directory, its checkout,
    /// that repository's worktrees; the click's own roots follow, deduped.
    #[test]
    fn a_document_roots_at_its_directory_checkout_and_worktrees() {
        let scratch = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(scratch.path()).unwrap();
        let repo = base.join("repo");
        std::fs::create_dir_all(repo.join("plans")).unwrap();
        std::fs::write(repo.join("plans/notes.md"), "n").unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "n"]);
        git(&repo, &["worktree", "add", "-q", "-b", "feat", base.join("repo-feat").to_str().unwrap()]);
        std::fs::create_dir_all(base.join("pane")).unwrap();

        let rest = vec![
            Root { dir: base.join("pane"), via: RootVia::PaneCwd },
            Root { dir: repo.clone(), via: RootVia::SessionCwd },
        ];
        let roots = doc_roots(&repo.join("plans/notes.md"), &rest);
        assert_eq!(
            rel(&roots, &base),
            vec![
                ("repo/plans".into(), RootVia::Document),
                ("repo".into(), RootVia::GitToplevel),
                ("repo-feat".into(), RootVia::Worktree("feat".into())),
                ("pane".into(), RootVia::PaneCwd),
            ]
        );
        // A document at the checkout root is its own toplevel.
        let at_root = doc_roots(&repo.join("README.md"), &[]);
        assert_eq!(
            rel(&at_root, &base),
            vec![("repo".into(), RootVia::Document), ("repo-feat".into(), RootVia::Worktree("feat".into()))]
        );
    }
}
