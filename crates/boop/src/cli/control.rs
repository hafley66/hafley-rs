//! `boop tui`: launch a harness's own interactive TUI, register the pane as
//! that harness's coordinator route, and project while it runs.

use std::collections::{BTreeSet, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use boop::bus::Route;
use boop::harness::{Harness, NativeTuiSpec};
use boop::registry::Registry;
use tracing::{info, warn};

use crate::cli::{mail_dir, write_route};

/// How long a fresh TUI is given to appear in its harness's own live-session
/// registry. The route is written either way; an unresolved session leaves the
/// `sessionId` field empty rather than carrying a guess.
const SESSION_WAIT: Duration = Duration::from_secs(10);

/// Respawn ceiling for one wrapper process; past it a dying TUI is a defect
/// to look at, never a loop to ride.
const RESPAWN_MAX: u32 = 3;

/// Below this uptime a nonzero exit is a launch defect (bad flag, bad
/// config) and a respawn would loop on it.
const RESPAWN_MIN_UPTIME: Duration = Duration::from_secs(10);

/// Whether a dead TUI earns another process against the same session. A
/// signal death is a deliberate kill and both gates below stop loops.
fn respawn_wanted(status: std::process::ExitStatus, respawns: u32, uptime: Duration) -> bool {
    status.code().is_some_and(|code| code != 0)
        && respawns < RESPAWN_MAX
        && uptime >= RESPAWN_MIN_UPTIME
}

/// The session this launch opened or resumed, read from the harness's own live
/// registry. A new root starts after `opened_ms`; an old root must be the only
/// same-cwd session whose observation advanced from the pre-launch snapshot.
fn opened_session(
    adapter: &dyn Harness,
    dir: &Path,
    cwd: &Path,
    opened_ms: u64,
    prior_observations: &HashMap<String, u64>,
    wait: Duration,
    my_pane: &str,
) -> Option<String> {
    let canonical = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let deadline = std::time::Instant::now() + wait;
    loop {
        let live = adapter.live().live_sessions().unwrap_or_default();
        let picked = newest_opened_session(
            live.clone(),
            &canonical,
            opened_ms,
            prior_observations,
            Some(my_pane),
            &claimed_sessions(dir, my_pane, &canonical),
        );
        if let Some(session) = picked {
            // The pane's own registry row is proof enough; a registry-derived
            // pick needs the claim marker.
            let exact = live
                .iter()
                .any(|held| held.session_id == session && held.tmux_pane.as_deref() == Some(my_pane));
            if exact || claim_open_session(dir, &session) {
                return Some(session);
            }
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn newest_opened_session(
    live: Vec<boop::live::LiveSession>,
    canonical_cwd: &Path,
    opened_ms: u64,
    prior_observations: &HashMap<String, u64>,
    my_pane: Option<&str>,
    claimed: &BTreeSet<String>,
) -> Option<String> {
    let candidates = live
        .into_iter()
        .filter(|session| {
            session
                .cwd
                .as_ref()
                .map(|dir| std::fs::canonicalize(dir).unwrap_or_else(|_| dir.clone()))
                .as_deref()
                == Some(canonical_cwd)
                && session.scope != boop::live::LiveSessionScope::Child
                && !claimed.contains(&session.session_id)
        })
        .collect::<Vec<_>>();
    // The pane's own registry row outranks the newest-session heuristic:
    // a pane names exactly one session.
    if let Some(pane) = my_pane {
        if let Some(session) = candidates
            .iter()
            .find(|session| session.tmux_pane.as_deref() == Some(pane))
        {
            return Some(session.session_id.clone());
        }
    }
    if let Some(session) = candidates
        .iter()
        .filter(|session| {
            session.started_ms.map_or_else(
                || {
                    session.observed_ms >= opened_ms
                        && !prior_observations.contains_key(&session.session_id)
                },
                |started| started >= opened_ms,
            )
        })
        .max_by_key(|session| session.observed_ms)
    {
        return Some(session.session_id.clone());
    }
    let mut resumed = candidates.into_iter().filter(|session| {
        session.observed_ms >= opened_ms
            && prior_observations
                .get(&session.session_id)
                .is_some_and(|prior| session.observed_ms > *prior)
    });
    let session = resumed.next()?;
    resumed.next().is_none().then_some(session.session_id)
}

/// Sessions other registry routes already carry in one cwd. A second
/// same-cwd wrapper must never bind a session a running pane owns.
fn claimed_sessions(dir: &Path, my_pane: &str, canonical_cwd: &Path) -> BTreeSet<String> {
    let Ok(routes) = boop::bus::read_routes(dir) else {
        return BTreeSet::new();
    };
    let mut claimed = BTreeSet::new();
    for route in routes.into_values() {
        let Some(session) = route.session_id else {
            continue;
        };
        let Some(cwd) = route.cwd.as_deref() else {
            continue;
        };
        let route_cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| PathBuf::from(cwd));
        if route_cwd != canonical_cwd {
            continue;
        }
        let pane = route
            .tmux
            .as_deref()
            .map(|target| boop::live::pane_of_target(target).unwrap_or_else(|| target.to_owned()));
        if pane.as_deref() == Some(my_pane) {
            continue;
        }
        claimed.insert(session);
    }
    claimed
}

/// One registry-derived bind takes an exclusive marker: two wrappers waking
/// on one poll tick take two sessions, one each. A pane-exact bind skips it.
fn claim_open_session(dir: &Path, session_id: &str) -> bool {
    prune_claims(dir);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(format!("tui-claim-{session_id}")))
        .is_ok()
}

/// Claim markers older than a day are dead wrappers' leftovers.
fn prune_claims(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("tui-claim-") {
            continue;
        }
        let age = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .map(|modified| modified.elapsed().unwrap_or_default());
        if age.is_ok_and(|age| age > Duration::from_secs(24 * 60 * 60)) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn session_observations(adapter: &dyn Harness) -> HashMap<String, u64> {
    adapter
        .live()
        .live_sessions()
        .unwrap_or_default()
        .into_iter()
        .map(|session| (session.session_id, session.observed_ms))
        .collect()
}

/// Holds the pane in the terminal's alternate screen for a harness whose own
/// TUI never asks for it. codex 0.151.0 parses `[tui] alternate_screen` and
/// then renders inline anyway (openai/codex#24552), so its repaints scroll into
/// tmux history and the input box walks up the pane instead of staying pinned.
/// Writing the switch here makes tmux flip the pane to its alternate buffer for
/// the child's whole life: input at the bottom, `history_size` stays 0. `Drop`
/// restores the primary screen on every exit path, bail included.
struct AlternateScreen;

impl AlternateScreen {
    fn enter(wanted: bool) -> Option<Self> {
        if !wanted {
            return None;
        }
        let mut out = std::io::stdout();
        out.write_all(b"\x1b[?1049h").ok()?;
        out.flush().ok()?;
        Some(Self)
    }
}

impl Drop for AlternateScreen {
    fn drop(&mut self) {
        let mut out = std::io::stdout();
        let _ = out.write_all(b"\x1b[?1049l");
        let _ = out.flush();
    }
}

/// Bind a tmux pane to the boop session running in it, so a reader with a pane
/// in hand can name the session outright instead of matching text against every
/// recent session of the harness.
fn record_pane(
    open: Option<&boop::Store>,
    session: &str,
    pid: u32,
    pane: &str,
) -> anyhow::Result<()> {
    let owned;
    let store = match open {
        Some(store) => store,
        None => {
            owned = boop::Store::open(boop::Store::default_path()?)?;
            &owned
        }
    };
    store.record_status(
        session,
        boop::live::now_ms(),
        "live",
        Some(i64::from(pid)),
        Some(pane),
    )
}

/// Ask the selected harness adapter to prepare its native process, register
/// this pane as its coordinator, then run the ordinary interactive TUI.
pub(crate) fn run_native_tui(
    registry: &Registry,
    adapter: &dyn Harness,
    name: Option<&str>,
    cwd: &Path,
    mail_dir_arg: Option<&Path>,
    executable: Option<&str>,
    tui_args: &[String],
) -> Result<()> {
    let pane = std::env::var("TMUX_PANE")
        .ok()
        .filter(|pane| !pane.is_empty())
        .context("`boop tui` requires TMUX_PANE so its route matches harness identity")?;
    let default_name = format!("{}-{}", adapter.id(), pane.trim_start_matches('%'));
    let name = name.unwrap_or(&default_name);
    anyhow::ensure!(
        name == default_name,
        "native {} route name must be {default_name} for its TMUX_PANE",
        adapter.id()
    );
    let executable = executable.unwrap_or(adapter.id().as_str());
    let mut plan = adapter.door().tui_launch(&NativeTuiSpec {
        executable: executable.into(),
        cwd: cwd.to_path_buf(),
        args: tui_args.to_vec(),
    })?;
    let dir = mail_dir(mail_dir_arg)?;
    let store = adapter
        .capabilities()
        .native_tui_projector
        .then(|| boop::Store::open(boop::Store::default_path()?))
        .transpose()?;
    let mut known = store
        .as_ref()
        .map(boop::Store::known_sessions)
        .transpose()?;
    let prior_observations = session_observations(adapter);
    let opened_ms = boop::live::now_ms();
    let _alternate_screen =
        AlternateScreen::enter(adapter.capabilities().wrapper_owns_alternate_screen);
    // The stamp every `boop` call inside this TUI reads as its identity,
    // inherited by the harness's own shell and native subagents.
    let mut child = Command::new(&plan.program)
        .args(&plan.args)
        .env("BOOP_SESSION", name)
        .env("BOOP_LANE", name)
        .current_dir(cwd)
        .spawn()
        .with_context(|| format!("start native {} TUI", adapter.id()))?;
    let mut respawns: u32 = 0;
    let mut spawned_at = std::time::Instant::now();
    if plan.session_id.is_none() {
        plan.session_id = opened_session(
            adapter,
            &dir,
            cwd,
            opened_ms,
            &prior_observations,
            SESSION_WAIT,
            &pane,
        );
        if let Some(session) = plan.session_id.as_deref() {
            plan.source_path = Some(format!("native-session={session}"));
            info!(%session, harness = %adapter.id(), "native session route resolved");
        }
    }
    // The pane id is the only thing tying this tmux cell to a boop session. It
    // went into the route file and nowhere else, so `agent_live` held 0 pane
    // ids across 4565 rows and any reader holding a pane had to guess its
    // session from a pool of every recent session of that harness.
    //
    // Opened separately from `store` above: registering a pane is required even
    // for a harness that opts out of resident transcript projection.
    if let Some(session) = plan.session_id.as_deref() {
        if let Err(error) = record_pane(store.as_ref(), session, child.id(), &pane) {
            eprintln!("boop: pane {pane} not recorded for session {session}: {error}");
        }
    }
    let mut route = Route {
        kind: "coordinator".into(),
        harness: Some(adapter.id()),
        tmux: Some(pane.clone()),
        cwd: Some(cwd.display().to_string()),
        model: None,
        mode: Some(plan.mode.clone()),
        session_id: plan.session_id.clone(),
        source_path: plan.source_path.clone(),
        parent: None,
        goal: None,
        registered_at: Some(boop::bus::now_iso()),
        base_sha: None,
        worktree_dir: None,
        app_server_socket: plan.app_server_socket.clone(),
    };
    write_route(&dir, name, route.clone())?;
    // Child exit observation stays responsive while transcript projection is
    // independently bounded. The global known-session join ran once above;
    // every pass below reuses and incrementally updates that resident cache.
    const EXIT_POLL: Duration = Duration::from_millis(250);
    let parent_project_every = std::env::var("BOOP_NATIVE_PROJECT_EVERY_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(1));
    let discover_every = std::env::var("BOOP_NATIVE_DISCOVER_EVERY_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(30));
    let mut last_parent_project = std::time::Instant::now() - parent_project_every;
    let mut last_discover = std::time::Instant::now() - discover_every;
    loop {
        if let Some(status) = child.try_wait().context("observe native TUI exit")? {
            if status.success() {
                return Ok(());
            }
            let next = match route.session_id.as_deref() {
                Some(session) if respawn_wanted(status, respawns, spawned_at.elapsed()) => {
                    adapter.door().tui_relaunch(
                        &NativeTuiSpec {
                            executable: executable.into(),
                            cwd: cwd.to_path_buf(),
                            args: Vec::new(),
                        },
                        session,
                    )?
                }
                _ => None,
            };
            let Some(next) = next else {
                anyhow::bail!("native {} TUI exited with {status}", adapter.id());
            };
            respawns += 1;
            warn!(
                route = name,
                status = %status,
                attempt = respawns,
                "native TUI died; respawning against its session"
            );
            child = Command::new(&next.program)
                .args(&next.args)
                .env("BOOP_SESSION", name)
                .env("BOOP_LANE", name)
                .current_dir(cwd)
                .spawn()
                .with_context(|| format!("respawn native {} TUI", adapter.id()))?;
            spawned_at = std::time::Instant::now();
            route.app_server_socket = next.app_server_socket.clone();
            route.source_path = next.source_path.clone();
            write_route(&dir, name, route.clone())?;
            continue;
        }
        // A fresh TUI opens its session at its first prompt, after the route
        // was written; the route learns the id the first tick it exists.
        if route.session_id.is_none() {
            match opened_session(
                adapter,
                &dir,
                cwd,
                opened_ms,
                &prior_observations,
                Duration::ZERO,
                &pane,
            ) {
                Some(session) => {
                    route.source_path = Some(format!("native-session={session}"));
                    route.session_id = Some(session.clone());
                    write_route(&dir, name, route.clone())?;
                    info!(route = name, %session, "native session route recovered after launch");
                    if let Err(error) = record_pane(store.as_ref(), &session, child.id(), &pane) {
                        warn!(%error, %pane, %session, "recovered native pane was not recorded");
                    }
                }
                None => {
                    std::thread::sleep(EXIT_POLL);
                    continue;
                }
            }
        }
        if let Some(store) = store.as_ref() {
            let known = known.as_mut().expect("projector store has a session cache");
            if last_discover.elapsed() >= discover_every {
                last_discover = std::time::Instant::now();
                if let Err(error) = crate::cli::db::sync_native_child_route_once(
                    store,
                    known,
                    adapter,
                    name,
                    &dir,
                    |message| crate::cli::mail::deliver_hail(registry, &dir, message, None),
                ) {
                    warn!(%error, route = name, "native discovery projector pass failed");
                }
            }
            if last_parent_project.elapsed() >= parent_project_every {
                last_parent_project = std::time::Instant::now();
                if let Err(error) =
                    crate::cli::db::sync_native_parent_route_once(store, known, adapter, name, &dir)
                {
                    warn!(%error, route = name, "native parent projector pass failed");
                }
            }
        }
        std::thread::sleep(EXIT_POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::{claim_open_session, newest_opened_session, respawn_wanted, RESPAWN_MIN_UPTIME};
    use std::collections::{BTreeSet, HashMap};
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;
    use std::time::Duration;

    #[test]
    fn nonzero_exit_after_min_uptime_respawns() {
        let status = ExitStatus::from_raw(256);
        assert!(respawn_wanted(status, 0, RESPAWN_MIN_UPTIME));
        assert!(respawn_wanted(status, 2, Duration::from_secs(3600)));
    }

    #[test]
    fn signal_death_fast_death_and_exhaustion_end_the_wrapper() {
        let killed = ExitStatus::from_raw(9);
        assert!(!respawn_wanted(killed, 0, Duration::from_secs(3600)));
        let failed = ExitStatus::from_raw(256);
        assert!(!respawn_wanted(failed, 0, Duration::from_secs(1)));
        assert!(!respawn_wanted(failed, 3, Duration::from_secs(3600)));
    }

    fn session(
        id: &str,
        started_ms: u64,
        scope: boop::live::LiveSessionScope,
    ) -> boop::live::LiveSession {
        boop::live::LiveSession {
            harness: boop::harness::HarnessId::Codex,
            session_id: id.to_owned(),
            pid: None,
            cwd: Some("/tmp/turn-attribution-parent".into()),
            tmux_pane: None,
            status: boop::live::LiveStatus::Unknown,
            door: boop::live::DoorAddress::None,
            observed_ms: started_ms,
            started_ms: Some(started_ms),
            scope,
            parent_session: None,
        }
    }

    #[test]
    fn a_guardian_started_111ms_later_does_not_take_the_parent_pane() {
        let root = session(
            "01a053e4-d12c-parent",
            1_788_113_899_820,
            boop::live::LiveSessionScope::Root,
        );
        let guardian = session(
            "01a053e4-d19b-guardian",
            1_788_113_899_931,
            boop::live::LiveSessionScope::Child,
        );
        assert_eq!(
            newest_opened_session(
                vec![root, guardian],
                std::path::Path::new("/tmp/turn-attribution-parent"),
                1_788_113_899_000,
                &HashMap::new(),
                None,
                &BTreeSet::new(),
            )
            .as_deref(),
            Some("01a053e4-d12c-parent")
        );
    }

    #[test]
    fn one_old_root_updated_after_launch_is_recovered() {
        let mut root = session(
            "01a0550d-old-root",
            1_788_100_000_000,
            boop::live::LiveSessionScope::Root,
        );
        root.observed_ms = 1_788_200_001_000;
        let prior = HashMap::from([(root.session_id.clone(), 1_788_199_999_000)]);

        assert_eq!(
            newest_opened_session(
                vec![root],
                std::path::Path::new("/tmp/turn-attribution-parent"),
                1_788_200_000_000,
                &prior,
                None,
                &BTreeSet::new(),
            )
            .as_deref(),
            Some("01a0550d-old-root")
        );
    }

    #[test]
    fn unchanged_or_ambiguous_old_roots_are_not_recovered() {
        let mut unchanged = session(
            "unchanged",
            1_788_100_000_000,
            boop::live::LiveSessionScope::Root,
        );
        unchanged.observed_ms = 1_788_199_999_000;
        let mut first = unchanged.clone();
        first.session_id = "first".into();
        first.observed_ms = 1_788_200_001_000;
        let mut second = first.clone();
        second.session_id = "second".into();
        let prior = HashMap::from([
            ("unchanged".into(), 1_788_199_999_000),
            ("first".into(), 1_788_199_999_000),
            ("second".into(), 1_788_199_999_000),
        ]);

        assert_eq!(
            newest_opened_session(
                vec![unchanged],
                std::path::Path::new("/tmp/turn-attribution-parent"),
                1_788_200_000_000,
                &prior,
                None,
                &BTreeSet::new(),
            ),
            None
        );
        assert_eq!(
            newest_opened_session(
                vec![first, second],
                std::path::Path::new("/tmp/turn-attribution-parent"),
                1_788_200_000_000,
                &prior,
                None,
                &BTreeSet::new(),
            ),
            None
        );
    }

    /// RECEIPT. Two same-cwd panes of one harness bound the newest session
    /// twice; a pane's own registry row outranks the newest-session heuristic.
    #[test]
    fn a_pane_exact_session_wins_over_a_newer_unpaned_one() {
        let mut own = session("own-pane", 1_000, boop::live::LiveSessionScope::Root);
        own.tmux_pane = Some("%415".into());
        let newer = session("newer", 2_000, boop::live::LiveSessionScope::Root);
        assert_eq!(
            newest_opened_session(
                vec![newer, own],
                std::path::Path::new("/tmp/turn-attribution-parent"),
                900,
                &HashMap::new(),
                Some("%415"),
                &BTreeSet::new(),
            )
            .as_deref(),
            Some("own-pane")
        );
    }

    /// RECEIPT. codex's registry records no pane; its binding is registry-derived.
    #[test]
    fn a_session_another_pane_claimed_is_never_taken_again() {
        let first = session("first", 1_000, boop::live::LiveSessionScope::Root);
        let second = session("second", 2_000, boop::live::LiveSessionScope::Root);
        let claimed = BTreeSet::from(["second".to_string()]);
        assert_eq!(
            newest_opened_session(
                vec![first, second],
                std::path::Path::new("/tmp/turn-attribution-parent"),
                900,
                &HashMap::new(),
                None,
                &claimed,
            )
            .as_deref(),
            Some("first")
        );
    }

    #[test]
    fn two_registry_derived_binds_take_two_sessions_one_each() {
        let dir = std::env::temp_dir().join(format!("boop-claim-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert!(claim_open_session(&dir, "aa"));
        assert!(!claim_open_session(&dir, "aa"));
        assert!(claim_open_session(&dir, "bb"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
