//! The click roots with the pane's agent sessions folded in: boop-mux's pane
//! roots, extended with the session cwds and the checkouts of touched files.

use std::path::{Path, PathBuf};

use boop_mux::{repo_root_for, PaneHit, Root};
use boop_store::SessionTouched;

/// The ordered, canonically deduped roots for a click in `pane`.
pub fn click_roots(pane: &PaneHit, touched: &SessionTouched) -> Vec<Root> {
    let cwds: Vec<PathBuf> = touched.cwds.iter().map(PathBuf::from).collect();
    let dirs: Vec<PathBuf> = touched
        .paths
        .iter()
        .filter_map(|path| Path::new(path).parent().map(|dir| dir.to_string_lossy().into_owned()))
        .map(|parent| PathBuf::from(repo_root_for(&parent).unwrap_or(parent)))
        .collect();
    boop_mux::click_roots(&pane.pane_current_path, &cwds, &dirs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use boop_mux::{worktrees_of, RootVia};

    fn git(dir: &Path, args: &[&str]) {
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
}
