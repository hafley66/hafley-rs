use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::info;

use boop::bus::Route;
use boop::harness::{HarnessId, MailPolicy};
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
    let harness = harness.map(str::parse::<HarnessId>).transpose()?;
    let discovered_session = match session_id {
        Some(session_id) => Some(session_id.to_owned()),
        None => live_session_id(registry, harness, multiplexer, tmux_session)?,
    };
    let route = Route {
        kind: kind.into(),
        harness,
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
        app_server_socket: None,
    };
    write_route(&dir, name, route)?;
    println!("adopted {name} -> tmux {tmux_session}");
    // A pane driven by a model between turns takes mail at a turn boundary;
    // every other harness keeps pane injection.
    let turn_boundary = harness
        .is_some_and(|id| registry.get(id).capabilities().mail == MailPolicy::TurnBoundaryHook);
    if turn_boundary && !hooks.no_hooks {
        let project = adopt_cwd(cwd)?;
        let changed = write_inbox_hooks(&project, name, false)?;
        report_inbox_hooks(&project, name, false, changed);
        println!("hails to {name} now queue for the hook inbox, never its keyboard");
    }
    Ok(())
}

/// The session the harness's own live registry reports in the adopted pane.
/// A target tmux cannot resolve to a pane, or a pane no session holds, leaves
/// the route anonymous rather than carrying a guess.
fn live_session_id(
    registry: &Registry,
    harness: Option<HarnessId>,
    multiplexer: &dyn tmux::Multiplexer,
    tmux_target: &str,
) -> Result<Option<String>> {
    let (Some(harness), Some(pane)) = (harness, adopt_pane(multiplexer, tmux_target)) else {
        return Ok(None);
    };
    Ok(registry
        .get(harness)
        .live()
        .live_session_in_pane(&pane)?
        .map(|session| session.session_id))
}

/// The pane id an adopt target names: written as one, or resolved by tmux.
fn adopt_pane(multiplexer: &dyn tmux::Multiplexer, target: &str) -> Option<String> {
    if target.starts_with('%') {
        return Some(target.to_owned());
    }
    boop::live::pane_of_target(target).or_else(|| multiplexer.pane_id(None, target))
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
            harness: Some(HarnessId::Codex),
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
            app_server_socket: None,
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

pub(crate) fn run_whoami(json: bool, mail_dir_arg: Option<&Path>) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
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
    use boop::harness::{Capabilities, Harness, LanePolicy, VariantSupport};
    use boop::tmux::{LiveSessions, Multiplexer};

    struct AdoptMux;

    impl Multiplexer for AdoptMux {
        fn current_pane(&self, _: Option<&str>) -> Option<String> {
            None
        }
        fn session_of_pane(&self, _: Option<&str>, _: &str) -> Option<String> {
            None
        }
        fn pane_id(&self, _: Option<&str>, target: &str) -> Option<String> {
            (target == "sprefa-5:0.0").then(|| "%77".to_owned())
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

    /// A claude harness whose live registry holds one session, in one pane.
    struct LiveClaude;

    static CLAUDE_CAPABILITIES: Capabilities = Capabilities {
        bans_plan_family_models: false,
        lanes: LanePolicy::CoordinatorSubagentsOnly,
        variant: VariantSupport::None,
        mail: MailPolicy::Door,
        native_tui_projector: false,
    };

    struct OnePane;

    impl boop::live::LiveSessions for OnePane {
        fn live_sessions(&self) -> anyhow::Result<Vec<boop::live::LiveSession>> {
            Ok(vec![boop::live::LiveSession {
                harness: HarnessId::Claude,
                session_id: "da6da0ca-5ad6-4f2f-88f7-de82e79f1e6b".into(),
                pid: Some(11),
                cwd: None,
                tmux_pane: Some("%77".into()),
                status: boop::live::LiveStatus::Idle,
                door: boop::live::DoorAddress::None,
                observed_ms: 1,
            }])
        }
    }

    impl Harness for LiveClaude {
        fn id(&self) -> HarnessId {
            HarnessId::Claude
        }

        fn capabilities(&self) -> &'static Capabilities {
            &CLAUDE_CAPABILITIES
        }

        fn live(&self) -> &dyn boop::live::LiveSessions {
            &OnePane
        }

        fn sessions(&self) -> anyhow::Result<Vec<boop::harness::SessionRef>> {
            Ok(Vec::new())
        }

        fn read_from(
            &self,
            _session: &boop::harness::SessionRef,
            offset: u64,
        ) -> anyhow::Result<boop::harness::ReadChunk> {
            Ok(boop::harness::ReadChunk {
                events: Vec::new(),
                next_offset: offset,
                reset: false,
                skipped: 0,
            })
        }
    }

    #[test]
    /// RECEIPT. Adopt names the session the harness's own live registry
    /// reports for the adopted pane, and an explicit `--session-id` still wins.
    fn adopt_reads_the_session_from_the_live_registry_and_explicit_id_wins() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mux = AdoptMux;
        let registry = Registry::with(vec![Box::new(LiveClaude)]);
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
            Some(SubCmd::Me {
                cmd: Some(MeCmd::Favorite { index: -1, .. }),
                ..
            })
        ));
    }

    #[test]
    fn me_favorite_accepts_an_older_negative_position() {
        let cli = Cli::try_parse_from(["boop", "me", "favorite", "-2", "--note", "keep"])
            .expect("negative favorite position parses");
        assert!(matches!(
            cli.command,
            Some(SubCmd::Me {
                cmd: Some(MeCmd::Favorite { index: -2, .. }),
                ..
            })
        ));
    }
}
