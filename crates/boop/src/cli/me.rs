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
