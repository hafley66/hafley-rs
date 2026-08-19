use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::info;

use boop::bus::Route;
use boop::registry::Registry;
use boop::{bus, ident, identity, tmux};

use crate::cli::db::open_store;
use crate::cli::job::waiting_as;
use crate::cli::mail::{report_inbox_hooks, write_inbox_hooks};
use crate::cli::{line, mail_dir, now_ms, write_route};

// ---------------------------------------------------------------------------
// adopt / prune
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
/// What an adopt does about the adopted session's hook inbox.
pub(crate) struct HookWiring {
    pub(crate) no_hooks: bool,
    pub(crate) uninstall: bool,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_adopt(
    name: &str,
    kind: &str,
    tmux_session: &str,
    harness: Option<&str>,
    session_id: Option<&str>,
    cwd: Option<&str>,
    model: Option<&str>,
    mode: Option<&str>,
    parent: Option<&str>,
    goal: Option<&str>,
    mail_dir_arg: Option<&Path>,
    hooks: HookWiring,
) -> Result<()> {
    let registry = Registry::discover();
    let processes = crate::proc::SysinfoSnapshot::capture()?;
    run_adopt_with(
        name,
        kind,
        tmux_session,
        harness,
        session_id,
        cwd,
        model,
        mode,
        parent,
        goal,
        mail_dir_arg,
        hooks,
        &registry,
        tmux::mux(),
        &processes,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_adopt_with(
    name: &str,
    kind: &str,
    tmux_session: &str,
    harness: Option<&str>,
    session_id: Option<&str>,
    cwd: Option<&str>,
    model: Option<&str>,
    mode: Option<&str>,
    parent: Option<&str>,
    goal: Option<&str>,
    mail_dir_arg: Option<&Path>,
    hooks: HookWiring,
    registry: &Registry,
    multiplexer: &dyn tmux::Multiplexer,
    processes: &dyn crate::proc::ProcReader,
) -> Result<()> {
    // Taking the hooks out is about a project directory, not about a pane, and
    // the pane is usually already gone by the time anyone wants that.
    if hooks.uninstall {
        let project = adopt_cwd(cwd)?;
        let changed = write_inbox_hooks(&project, name, true)?;
        report_inbox_hooks(&project, name, true, changed);
        return Ok(());
    }
    if !multiplexer.has_session(None, tmux_session)? {
        println!("refusing adopt {name}: no such tmux session {tmux_session}");
        return Ok(());
    }
    let dir = mail_dir(mail_dir_arg)?;
    let existing = bus::read_routes(&dir)?.remove(name);
    let discovered_session = session_id.map(str::to_owned).or_else(|| {
        harness.and_then(|id| {
            registry.by_id(id).and_then(|adapter| {
                adapter.session_id_in_pane(multiplexer, processes, tmux_session)
            })
        })
    });
    let route = Route {
        kind: kind.into(),
        harness: harness.map(str::to_owned),
        tmux: Some(tmux_session.to_owned()),
        cwd: cwd.map(str::to_owned),
        model: model.map(str::to_owned),
        mode: mode.map(str::to_owned),
        session_id: discovered_session.or_else(|| existing.and_then(|route| route.session_id)),
        source_path: None,
        parent: parent.map(str::to_owned),
        goal: goal.map(str::to_owned),
        registered_at: Some(bus::now_iso()),
        base_sha: None,
        worktree_dir: None,
    };
    write_route(&dir, name, route)?;
    println!("adopted {name} -> tmux {tmux_session}");
    // A claude pane is driven by a model between turns, so mail belongs at a
    // turn boundary; every other harness keeps pane injection.
    let claude = harness == Some("claude");
    if claude && !hooks.no_hooks {
        let project = adopt_cwd(cwd)?;
        let changed = write_inbox_hooks(&project, name, false)?;
        report_inbox_hooks(&project, name, false, changed);
        println!("hails to {name} now queue for the hook inbox, never its keyboard");
    }
    Ok(())
}

/// The project directory whose settings carry an adopted session's hooks.
pub(crate) fn adopt_cwd(cwd: Option<&str>) -> Result<PathBuf> {
    match cwd {
        Some(cwd) => Ok(PathBuf::from(cwd)),
        None => std::env::current_dir().context("read the current directory"),
    }
}

pub(crate) fn run_prune(mail_dir_arg: Option<&Path>) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    if tmux::mux().live_sessions(None).is_none() {
        println!("refusing prune: tmux unreachable, cannot tell live from dead");
        return Ok(());
    }
    let routes = bus::read_routes(&dir)?;
    let dead: Vec<String> = routes
        .iter()
        .filter(|(_, route)| route.kind == "lane")
        .filter(|(_, route)| {
            let Some(target) = route.tmux.as_deref() else {
                return true;
            };
            !tmux::mux().target_alive(None, target)
        })
        .map(|(name, _)| name.clone())
        .collect();
    let path = dir.join("registry.json");
    bus::cas_update_json(&path, |current| {
        for name in &dead {
            current.remove(name);
        }
        Ok(())
    })?;
    info!(routes_deleted = dead.len(), mail_dir = %dir.display(), "lane routes pruned");
    println!("pruned {} dead routes", dead.len());
    Ok(())
}

// ---------------------------------------------------------------------------
// whoami
// ---------------------------------------------------------------------------

pub(crate) fn run_me_favorite(index: i64, note: Option<&str>) -> Result<()> {
    anyhow::ensure!(
        index < 0,
        "favorite index must be negative; -1 is the newest assistant message"
    );

    let dir = mail_dir(None)?;
    let routes = bus::read_routes(&dir).unwrap_or_default();
    let identity = identity::resolve(&routes)?;
    let session = identity
        .session
        .context("no caller session resolved; run `boop me` once in this tmux pane, then retry")?;

    let store = open_store()?;
    let rows = store.turn_rows(&ident::TurnQuery {
        session: Some(session.clone()),
        role: Some("assistant".to_owned()),
        ..Default::default()
    })?;
    let offset = index
        .checked_neg()
        .and_then(|value| value.checked_sub(1))
        .context("favorite index is outside the supported range")? as usize;
    let row = rows.iter().rev().nth(offset).with_context(|| {
        format!(
            "session {session} has {} assistant messages; cannot select {index}",
            rows.len()
        )
    })?;
    anyhow::ensure!(
        !row.said.trim().is_empty(),
        "selected assistant message is empty"
    );
    let source = format!("{}:{}:assistant:{}", row.harness, session, row.turn);
    let id = store.favorite_add(&row.said, note.unwrap_or(""), &source, now_ms())?;
    line(&format!("favorite {id}"));
    Ok(())
}

pub(crate) fn run_me(name: Option<&str>, mail_dir_arg: Option<&Path>) -> Result<()> {
    let pane = std::env::var("TMUX_PANE")
        .ok()
        .filter(|pane| !pane.is_empty())
        .or_else(|| tmux::mux().current_pane(None))
        .context("resolve current tmux pane; run boop me inside tmux")?;
    let cwd = std::env::current_dir().context("read current directory")?;
    let session = boop::harness::codex::latest_root_session_for_cwd(&cwd)?
        .context("no root Codex transcript records the current directory")?;
    let generated = format!("codex-{}", pane.trim_start_matches('%'));
    let name = name.unwrap_or(&generated);
    let dir = mail_dir(mail_dir_arg)?;
    write_route(
        &dir,
        name,
        Route {
            kind: "coordinator".into(),
            harness: Some("codex".into()),
            tmux: Some(pane.clone()),
            cwd: Some(cwd.display().to_string()),
            model: None,
            mode: Some("interactive".into()),
            session_id: Some(session.session_id.clone()),
            source_path: Some(session.path.display().to_string()),
            parent: None,
            goal: None,
            registered_at: Some(bus::now_iso()),
            base_sha: None,
            worktree_dir: None,
        },
    )?;
    println!("registered {name} -> {pane} codex {}", session.session_id);
    if let Ok(mood) = boop::Store::default_path()
        .and_then(boop::Store::open)
        .and_then(|store| store.effective_mood(name))
    {
        println!("{}", mood.line());
    }
    Ok(())
}

/// Read or write the caller's mood. Writing validates the name against the
/// stored moods, so a typo never reaches a delivery path.
pub(crate) fn run_me_mood(
    mood: Option<&str>,
    clear: bool,
    as_name: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let session = waiting_as(&dir, as_name)?;
    let store = boop::Store::open(boop::Store::default_path()?)?;
    match (mood, clear) {
        (Some(mood), _) => {
            store.set_session_mood(&session, mood, boop::channel::now_ms())?;
            println!("mood: {mood} (set on {session})");
        }
        (None, true) => {
            let had = store.clear_session_attr(&session, boop::ident::MOOD_ATTR_KEY)?;
            match had {
                true => println!("mood cleared on {session}"),
                false => println!("{session} had no mood of its own"),
            }
            println!("{}", store.effective_mood(&session)?.line());
        }
        (None, false) => println!("{}", store.effective_mood(&session)?.line()),
    }
    Ok(())
}

pub(crate) fn run_whoami(json: bool) -> Result<()> {
    let dir = mail_dir(None)?;
    let routes = bus::read_routes(&dir).unwrap_or_default();
    let identity = identity::resolve(&routes)?;
    if json {
        println!("{}", identity.to_json());
        return Ok(());
    }
    let rung = identity.rung.unwrap_or(identity::Rung::None);
    println!("session  {}", identity.session.as_deref().unwrap_or("-"));
    println!("lane     {}", identity.lane.as_deref().unwrap_or("-"));
    println!("parent   {}", identity.parent.as_deref().unwrap_or("-"));
    println!("harness  {}", identity.harness.as_deref().unwrap_or("-"));
    println!("pane     {}", identity.pane.as_deref().unwrap_or("-"));
    println!("rung     {} ({})", rung.as_str(), rung.confidence());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    use crate::cli::testkit::temp_mail_dir;
    use crate::{Cli, MeCmd, SubCmd};
    use boop::bus::read_routes;
    use boop::proc::{ProcReader, ProcessInfo};
    use boop::tmux::{LiveSessions, Multiplexer};

    struct ClaudeProcessFixture;

    struct AdoptMux;

    impl Multiplexer for AdoptMux {
        fn current_pane(&self, _: Option<&str>) -> Option<String> {
            None
        }
        fn session_of_pane(&self, _: Option<&str>, _: &str) -> Option<String> {
            None
        }
        fn pane_pid(&self, _: Option<&str>, _: &str) -> Option<u32> {
            Some(10)
        }
        fn live_sessions(&self, _: Option<&str>) -> Option<LiveSessions> {
            Some(LiveSessions {
                names: ["sprefa-5".into()].into_iter().collect(),
            })
        }
        fn has_session(&self, _: Option<&str>, target: &str) -> anyhow::Result<bool> {
            Ok(target.split(':').next() == Some("sprefa-5"))
        }
        fn kill_session(&self, _: Option<&str>, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn target_alive(&self, _: Option<&str>, _: &str) -> bool {
            true
        }
        fn capture_pane(&self, _: Option<&str>, _: &str, _: Option<u32>) -> anyhow::Result<String> {
            Ok(String::new())
        }
        fn new_detached_session(
            &self,
            _: Option<&str>,
            _: &str,
            _: &str,
            _: &str,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        fn new_bare_session(&self, _: Option<&str>, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn send_keys_literal(&self, _: Option<&str>, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn send_text(&self, _: Option<&str>, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn send_key_named(&self, _: Option<&str>, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn new_window(
            &self,
            _: Option<&str>,
            _: &str,
            _: &str,
            _: &str,
            _: &str,
        ) -> anyhow::Result<String> {
            Ok(String::new())
        }
        fn swap_windows(&self, _: Option<&str>, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn kill_window(&self, _: Option<&str>, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    impl ProcReader for ClaudeProcessFixture {
        fn is_alive(&self, pid: u32) -> bool {
            pid == 10 || pid == 11
        }
        fn process(&self, pid: u32) -> Option<ProcessInfo> {
            match pid {
                10 => Some(ProcessInfo {
                    pid,
                    parent: None,
                    name: "shell".into(),
                    command: vec!["zsh".into()],
                    rss_bytes: 0,
                    cpu_percent: 0.0,
                    start_time_secs: 0,
                    cwd: None,
                }),
                11 => Some(ProcessInfo {
                    pid,
                    parent: Some(10),
                    name: "claude".into(),
                    command: vec![
                        "claude".into(),
                        "--resume".into(),
                        "da6da0ca-5ad6-4f2f-88f7-de82e79f1e6b".into(),
                    ],
                    rss_bytes: 0,
                    cpu_percent: 0.0,
                    start_time_secs: 0,
                    cwd: None,
                }),
                _ => None,
            }
        }
        fn children(&self, pid: u32) -> Vec<u32> {
            (pid == 10).then_some(11).into_iter().collect()
        }
        fn descendants(&self, pid: u32) -> Vec<u32> {
            (pid == 10).then_some(11).into_iter().collect()
        }
        fn descendant_count(&self, pid: u32) -> usize {
            usize::from(pid == 10)
        }
    }

    #[test]
    fn adopt_discovers_claude_resume_identity_and_explicit_id_wins() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mux = AdoptMux;
        let processes = ClaudeProcessFixture;
        let registry = Registry::discover();
        let hooks = || HookWiring {
            no_hooks: true,
            uninstall: false,
        };

        run_adopt_with(
            "sprefa-coordinator",
            "coordinator",
            "sprefa-5:0.0",
            Some("claude"),
            None,
            Some("/repo"),
            None,
            None,
            None,
            None,
            Some(&dir),
            hooks(),
            &registry,
            &mux,
            &processes,
        )
        .unwrap();
        let discovered = read_routes(&dir).unwrap();
        assert_eq!(
            discovered["sprefa-coordinator"].session_id.as_deref(),
            Some("da6da0ca-5ad6-4f2f-88f7-de82e79f1e6b")
        );

        run_adopt_with(
            "sprefa-coordinator",
            "coordinator",
            "sprefa-5:0.0",
            Some("claude"),
            Some("explicit-session"),
            Some("/repo"),
            None,
            None,
            None,
            None,
            Some(&dir),
            hooks(),
            &registry,
            &mux,
            &processes,
        )
        .unwrap();
        let explicit = read_routes(&dir).unwrap();
        assert_eq!(
            explicit["sprefa-coordinator"].session_id.as_deref(),
            Some("explicit-session")
        );
        assert_eq!(explicit.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn me_favorite_defaults_to_the_newest_assistant_message() {
        let cli = Cli::try_parse_from(["boop", "me", "favorite"])
            .expect("caller-relative favorite command parses");
        assert!(matches!(
            cli.command,
            SubCmd::Me {
                cmd: Some(MeCmd::Favorite { index: -1, .. }),
                ..
            }
        ));
    }

    #[test]
    fn me_favorite_accepts_an_older_negative_position() {
        let cli = Cli::try_parse_from(["boop", "me", "favorite", "-2", "--note", "keep"])
            .expect("negative favorite position parses");
        assert!(matches!(
            cli.command,
            SubCmd::Me {
                cmd: Some(MeCmd::Favorite { index: -2, .. }),
                ..
            }
        ));
    }
}
