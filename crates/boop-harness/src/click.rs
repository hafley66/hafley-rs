//! ⌘-click resolution for a host terminal: client cell -> tmux pane -> the
//! boop sessions run in its cwd -> click roots -> the resolver ladder.

pub mod _0_rungs;
pub mod _1_roots;
pub mod _2_ladder;

use std::path::PathBuf;
use std::time::Instant;

use boop_mux::{Multiplexer, PaneHit, Tmux};
use boop_store::{SessionTouched, Store};
use serde::Deserialize;

pub use _2_ladder::{evidence_dirs, resolve, resolve_in_doc, AgentEvidence, ResolveResult, ResolvedRef};
pub use _1_roots::{click_roots, doc_roots, worktrees_of, Root, RootVia};
pub use _0_rungs::{clear_index_cache, git_out, home_dir, repo_root_of};

const TOUCHED_CAP: usize = 2000;
const CWD_SESSIONS: usize = 3;

/// The client cell a host saw the click on, in the tmux client's own grid
/// (zero-based, status line included), and the session that client shows.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ClickCell {
    pub session: String,
    pub socket: Option<String>,
    pub col: u16,
    pub row: u16,
}

#[derive(Debug)]
pub struct ClickResolution {
    pub result: ResolveResult,
    pub pane: PaneHit,
    pub sessions: Vec<String>,
    pub roots: Vec<Root>,
    pub evidence_paths: usize,
}

/// The caller's sessions, else those most recently run in the pane cwd. Registries
/// key panes by bare id, which collides across tmux servers.
fn click_sessions(pane: &PaneHit, given: &[String], store: Option<&Store>) -> Vec<String> {
    if !given.is_empty() {
        return given.to_vec();
    }
    let cwd = pane.pane_current_path.to_string_lossy();
    store.and_then(|store| store.sessions_in_cwd(&cwd, CWD_SESSIONS).ok()).unwrap_or_default()
}

/// Resolve `token` clicked at `cell`. A cell tmux cannot place (no server, a
/// border) falls back to `cwd` as the pane cwd. A token written in a document
/// (`doc`, the markdown file's path) resolves from that document's roots first.
pub fn resolve_click(
    token: &str,
    cell: Option<&ClickCell>,
    cwd: &str,
    sessions: &[String],
    doc: Option<&str>,
) -> ClickResolution {
    let started = Instant::now();
    let socket = cell.and_then(|cell| cell.socket.as_deref());
    let pane = cell
        .and_then(|cell| Tmux.pane_at(socket, &cell.session, cell.col, cell.row))
        .unwrap_or_else(|| PaneHit {
            pane: String::new(),
            pane_current_path: PathBuf::from(cwd),
            pane_col: 0,
            pane_row: 0,
        });
    let store = Store::default_path().ok().and_then(|path| Store::open_readonly(path).ok());
    let sessions = click_sessions(&pane, sessions, store.as_ref());
    let touched = store
        .as_ref()
        .and_then(|store| store.session_touched(&sessions, TOUCHED_CAP).ok())
        .unwrap_or_else(SessionTouched::default);
    let home = home_dir();
    let evidence = AgentEvidence::from_touched(&touched, &home);
    let roots = click_roots(&pane, &touched);
    let (roots, result) = match doc.map(std::path::Path::new) {
        Some(doc) => {
            let result = resolve_in_doc(token, doc, &roots, &home, &evidence);
            (doc_roots(doc, &roots), result)
        }
        None => {
            let result = resolve(token, &roots, &home, &evidence);
            (roots, result)
        }
    };
    let (kind, path, source, via) = match &result {
        ResolveResult::Hit { reference } => ("hit", reference.path.clone(), reference.source, ""),
        ResolveResult::Choices { paths, via, .. } => ("choices", paths.first().cloned().unwrap_or_default(), "", *via),
        ResolveResult::Absent { repo, rev, .. } => ("absent", format!("{repo}@{rev}"), "", ""),
        ResolveResult::Miss => ("miss", String::new(), "", ""),
    };
    tracing::info!(
        token,
        doc,
        pane = pane.pane,
        cwd = %pane.pane_current_path.display(),
        sessions = ?sessions,
        roots = roots.len(),
        evidence_paths = touched.paths.len(),
        result = kind,
        path,
        source,
        via,
        ms = started.elapsed().as_millis() as u64,
        "resolve_ref"
    );
    ClickResolution { result, pane, sessions, roots, evidence_paths: touched.paths.len() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
        assert!(status.success(), "git {args:?}");
    }

    fn pane(dir: &Path) -> PaneHit {
        PaneHit { pane: String::new(), pane_current_path: dir.to_path_buf(), pane_col: 0, pane_row: 0 }
    }

    fn rel(base: &Path, result: ResolveResult) -> ResolveResult {
        let strip = |path: String| path.strip_prefix(&format!("{}/", base.display())).unwrap_or(&path).to_owned();
        match result {
            ResolveResult::Hit { reference } => {
                ResolveResult::Hit { reference: ResolvedRef { path: strip(reference.path), ..reference } }
            }
            ResolveResult::Choices { paths, line, via, worktrees } => {
                ResolveResult::Choices { paths: paths.into_iter().map(strip).collect(), line, via, worktrees }
            }
            other => other,
        }
    }

    /// RECEIPT. The reported defect: an agent printed a repo-relative
    /// directory; the clicked pane's cwd resolves it as a directory hit.
    #[test]
    fn a_relative_directory_resolves_from_the_clicked_pane() {
        let scratch = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(scratch.path()).unwrap();
        let lab = "labs/20260924.0.the-gang-runs-a-program-as-data-through-differential-dataflow";
        std::fs::create_dir_all(base.join("sqlite_ivm").join(lab)).unwrap();
        std::fs::create_dir_all(base.join("sqlite_ivm/.git")).unwrap();
        std::fs::create_dir_all(base.join("instant/.git")).unwrap();
        let home = base.to_string_lossy().into_owned();

        let roots = click_roots(&pane(&base.join("sqlite_ivm")), &SessionTouched::default());
        let hit = resolve(lab, &roots, &home, &AgentEvidence::default());
        assert_eq!(
            rel(&base, hit),
            ResolveResult::Hit { reference: ResolvedRef { path: format!("sqlite_ivm/{lab}"), line: None, source: "cwd" } }
        );

        // From another pane's repo, the session's recorded cwd is the root that reaches it.
        let touched = SessionTouched { paths: Vec::new(), cwds: vec![base.join("sqlite_ivm").display().to_string()] };
        let roots = click_roots(&pane(&base.join("instant")), &touched);
        let hit = resolve(lab, &roots, &home, &AgentEvidence::default());
        assert_eq!(
            rel(&base, hit),
            ResolveResult::Hit { reference: ResolvedRef { path: format!("sqlite_ivm/{lab}"), line: None, source: "session" } }
        );
    }

    /// RECEIPT. Inline code in a markdown file names paths the way the file's
    /// author sees them: beside the file, from its checkout, in another
    /// worktree of that checkout, or a bare filename found once in it. Every
    /// line form opens at its first line. The pane sits in an unrelated repo.
    #[test]
    fn a_ref_in_a_document_resolves_from_the_document() {
        let scratch = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(scratch.path()).unwrap();
        let repo = base.join("hafley");
        for file in [
            "crates/scm/src/lang/rust/2_call.rs",
            "crates/scm/src/rust_modules.rs",
            "docs/plans/notes.md",
            "docs/plans/sibling.md",
        ] {
            std::fs::create_dir_all(repo.join(file).parent().unwrap()).unwrap();
            std::fs::write(repo.join(file), "x").unwrap();
        }
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "x"]);
        git(&repo, &["worktree", "add", "-q", "-b", "feat", base.join("hafley-feat").to_str().unwrap()]);
        std::fs::write(base.join("hafley-feat/only-feat.rs"), "f").unwrap();
        std::fs::create_dir_all(base.join("elsewhere/.git")).unwrap();
        std::fs::write(base.join("elsewhere/main.rs"), "m").unwrap();
        let home = base.to_string_lossy().into_owned();
        let doc = repo.join("docs/plans/notes.md");
        let roots = click_roots(&pane(&base.join("elsewhere")), &SessionTouched::default());

        let hit = |path: &str, line: Option<u32>, source: &'static str| ResolveResult::Hit {
            reference: ResolvedRef { path: path.into(), line, source },
        };
        let cases = [
            ("sibling.md", hit("hafley/docs/plans/sibling.md", None, "doc")),
            ("crates/scm/src/lang/rust/2_call.rs:790-801", hit("hafley/crates/scm/src/lang/rust/2_call.rs", Some(790), "repo")),
            ("2_call.rs:183-198", hit("hafley/crates/scm/src/lang/rust/2_call.rs", Some(183), "search")),
            ("2_call.rs:561,583", hit("hafley/crates/scm/src/lang/rust/2_call.rs", Some(561), "search")),
            ("rust_modules.rs:1105-1136", hit("hafley/crates/scm/src/rust_modules.rs", Some(1105), "search")),
            ("scm/src/lang/rust/2_call.rs:12", hit("hafley/crates/scm/src/lang/rust/2_call.rs", Some(12), "search")),
            ("crates/scm", hit("hafley/crates/scm", None, "repo")),
            ("only-feat.rs:3", hit("hafley-feat/only-feat.rs", Some(3), "worktree")),
            ("main.rs", hit("elsewhere/main.rs", None, "cwd")),
        ];
        for (token, want) in cases {
            clear_index_cache();
            let got = rel(&base, resolve_in_doc(token, &doc, &roots, &home, &AgentEvidence::default()));
            assert_eq!(got, want, "{token}");
        }
        // Without the document the pane's repo is the only anchor.
        clear_index_cache();
        assert_eq!(resolve("sibling.md", &roots, &home, &AgentEvidence::default()), ResolveResult::Miss);
    }

    /// RECEIPT. A path the pane's checkout lacks, present under several other
    /// worktrees of the same repository, is a choice tagged by branch.
    #[test]
    fn a_token_under_several_worktrees_is_a_tagged_choice() {
        let scratch = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(scratch.path()).unwrap();
        let repo = base.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("README.md"), "r").unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "r"]);
        for branch in ["alpha", "beta", "gamma"] {
            git(&repo, &["worktree", "add", "-q", "-b", branch, base.join(format!("wt-{branch}")).to_str().unwrap()]);
        }
        std::fs::create_dir_all(base.join("wt-alpha/plans")).unwrap();
        std::fs::write(base.join("wt-alpha/plans/x.md"), "a").unwrap();
        std::fs::create_dir_all(base.join("wt-gamma/plans")).unwrap();
        std::fs::write(base.join("wt-gamma/plans/x.md"), "g").unwrap();
        std::fs::write(base.join("wt-beta/only-beta.md"), "b").unwrap();
        let home = base.to_string_lossy().into_owned();
        let roots = click_roots(&pane(&repo), &SessionTouched::default());

        assert_eq!(
            rel(&base, resolve("plans/x.md:4", &roots, &home, &AgentEvidence::default())),
            ResolveResult::Choices {
                paths: vec!["wt-alpha/plans/x.md".into(), "wt-gamma/plans/x.md".into()],
                line: Some(4),
                via: "worktree",
                worktrees: vec!["alpha".into(), "gamma".into()],
            }
        );
        assert_eq!(
            rel(&base, resolve("only-beta.md", &roots, &home, &AgentEvidence::default())),
            ResolveResult::Hit { reference: ResolvedRef { path: "wt-beta/only-beta.md".into(), line: None, source: "worktree" } }
        );
        // The pane's own checkout wins over every other worktree.
        assert_eq!(
            rel(&base, resolve("README.md", &roots, &home, &AgentEvidence::default())),
            ResolveResult::Hit { reference: ResolvedRef { path: "repo/README.md".into(), line: None, source: "cwd" } }
        );
        assert_eq!(
            serde_json::to_string(&resolve("plans/x.md", &roots, &home, &AgentEvidence::default())).unwrap().replace(&home, ""),
            r#"{"kind":"choices","paths":["/wt-alpha/plans/x.md","/wt-gamma/plans/x.md"],"via":"worktree","worktrees":["alpha","gamma"]}"#
        );
    }
}
