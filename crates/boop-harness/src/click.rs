//! ⌘-click resolution for a host terminal: client cell -> tmux pane -> the
//! boop sessions in it -> click roots -> the resolver ladder.

pub mod _0_rungs;
pub mod _1_roots;
pub mod _2_ladder;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use boop_mux::{Multiplexer, PaneHit, Tmux};
use boop_store::{SessionTouched, Store};
use serde::Deserialize;

pub use _2_ladder::{evidence_dirs, resolve, AgentEvidence, ResolveResult, ResolvedRef};
pub use _1_roots::{click_roots, worktrees_of, Root, RootVia};
pub use _0_rungs::{clear_index_cache, git_out, home_dir, repo_root_of};

const TOUCHED_CAP: usize = 2000;
const PANE_SESSION_TTL: Duration = Duration::from_secs(5);
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

type PaneSessionCache = Mutex<HashMap<(Option<String>, String), (Instant, Option<String>)>>;

/// The boop session standing in `pane`, from the harness live registries.
/// Cached briefly: each ask walks every registry and may spawn a harness probe.
pub fn pane_session(pane: &str, socket: Option<&str>) -> Option<String> {
    static CACHE: OnceLock<PaneSessionCache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (socket.map(str::to_owned), pane.to_owned());
    if let Some((at, session)) = cache.lock().ok().and_then(|cache| cache.get(&key).cloned()) {
        if at.elapsed() < PANE_SESSION_TTL {
            return session;
        }
    }
    let registry = crate::Registry::discover();
    let session = boop_store::bus::default_mail_dir()
        .ok()
        .and_then(|mail| crate::live::session_in_pane_on_socket(&registry, pane, socket, &mail).ok().flatten());
    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, (Instant::now(), session.clone()));
    }
    session
}

/// Sessions whose evidence counts: the caller's, the one live in the pane,
/// and when neither names one, the sessions most recently run in the pane cwd.
fn click_sessions(pane: &PaneHit, socket: Option<&str>, given: &[String], store: Option<&Store>) -> Vec<String> {
    let mut sessions: Vec<String> = given.to_vec();
    if !pane.pane.is_empty() {
        if let Some(live) = pane_session(&pane.pane, socket) {
            if !sessions.contains(&live) {
                sessions.push(live);
            }
        }
    }
    if sessions.is_empty() {
        if let Some(store) = store {
            let cwd = pane.pane_current_path.to_string_lossy();
            sessions = store.sessions_in_cwd(&cwd, CWD_SESSIONS).unwrap_or_default();
        }
    }
    sessions
}

/// Resolve `token` clicked at `cell`. A cell tmux cannot place (no server, a
/// border) falls back to `cwd` as the pane cwd.
pub fn resolve_click(token: &str, cell: Option<&ClickCell>, cwd: &str, sessions: &[String]) -> ClickResolution {
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
    let sessions = click_sessions(&pane, socket, sessions, store.as_ref());
    let touched = store
        .as_ref()
        .and_then(|store| store.session_touched(&sessions, TOUCHED_CAP).ok())
        .unwrap_or_else(SessionTouched::default);
    let home = home_dir();
    let roots = click_roots(&pane, &touched);
    let result = resolve(token, &roots, &home, &AgentEvidence::from_touched(&touched, &home));
    let (kind, path, source, via) = match &result {
        ResolveResult::Hit { reference } => ("hit", reference.path.clone(), reference.source, ""),
        ResolveResult::Choices { paths, via, .. } => ("choices", paths.first().cloned().unwrap_or_default(), "", *via),
        ResolveResult::Absent { repo, rev, .. } => ("absent", format!("{repo}@{rev}"), "", ""),
        ResolveResult::Miss => ("miss", String::new(), "", ""),
    };
    tracing::info!(
        token,
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
