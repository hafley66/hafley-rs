use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use tracing::{debug, info};

use boop::bus::Route;
use boop::door::Delivered;
use boop::harness::HarnessId;
use boop::mail::{Landing, Via};
use boop::mailwait::Watch;
use boop::registry::Registry;
use boop::{bus, identity, inbox, lane, tmux};

use crate::cli::job::{wait_and_exit, waiting_as};
use crate::cli::{append_acks, append_message, append_message_to, line, mail_dir, pad};
use crate::InboxCmd;

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

pub(crate) fn run_list(mail_dir_arg: Option<&Path>, agent: Option<&str>, all: bool) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    match agent {
        None => {
            let routes = bus::read_routes(&dir)?;
            let live = tmux::mux().live_sessions(None);
            for (name, route) in &routes {
                let state = match &live {
                    None => "?",
                    Some(sessions) if sessions.has(route.tmux.as_deref().unwrap_or("")) => "live",
                    Some(_) => "dead",
                };
                let padded_name = pad(name, 16);
                let padded_harness = pad(route.harness.map_or("-", HarnessId::as_str), 10);
                let padded_mode = pad(route.mode.as_deref().unwrap_or("-"), 6);
                let padded_model = pad(route.model.as_deref().unwrap_or("-"), 46);
                let padded_tmux = pad(route.tmux.as_deref().unwrap_or("-"), 16);
                line(&format!(
                    "{} {} {} {} {} {} {} {}",
                    pad(state, 4),
                    padded_name,
                    pad(&route.kind, 12),
                    padded_harness,
                    padded_mode,
                    padded_model,
                    padded_tmux,
                    route.cwd.as_deref().unwrap_or("-"),
                ));
            }
            let messages = all_messages(&dir)?;
            let rows = if all {
                bus::fold(&messages)
            } else {
                bus::unacked(&messages)
            };
            for message in rows {
                line(&bus::message_line(&message).to_string());
            }
            if !all {
                line(&format!(
                    "{} open (closed history: --all)",
                    bus::unacked(&all_messages(&dir)?).len()
                ));
            }
        }
        Some(agent_id) => {
            let messages = all_messages(&dir)?;
            let rows = bus::fold(&messages);
            let inbox: Vec<_> = rows.iter().filter(|m| m.to == agent_id).cloned().collect();
            let outbox: Vec<_> = rows
                .iter()
                .filter(|m| m.from == agent_id)
                .cloned()
                .collect();
            for message in &inbox {
                line(&format!("in  {}", bus::message_line(message)));
            }
            for message in &outbox {
                line(&format!("out {}", bus::message_line(message)));
            }
            let mut combined = inbox.clone();
            combined.extend(outbox.iter().cloned());
            line(&format!(
                "{agent_id}: {} in, {} out, {} unacked",
                inbox.len(),
                outbox.len(),
                bus::unacked(&combined).len()
            ));
        }
    }
    Ok(())
}

pub(crate) fn all_messages(dir: &std::path::Path) -> Result<Vec<bus::Message>> {
    let mut messages = Vec::new();
    for path in bus::read_boxes(dir)? {
        messages.extend(bus::parse_box(&path));
    }
    Ok(messages)
}

// ---------------------------------------------------------------------------
// hail
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_hail(
    registry: &Registry,
    to: &str,
    body: &str,
    from: Option<&str>,
    kind: Option<&str>,
    box_name: Option<&str>,
    socket: Option<&str>,
    wait_timeout: Option<u64>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let message = bus::Message {
        id: bus::mint_id(),
        from: from.unwrap_or("coordinator").to_owned(),
        to: to.to_owned(),
        from_timestamp: bus::now_iso(),
        to_timestamp: None,
        kind: kind.unwrap_or("request").to_owned(),
        reply_to: None,
        body: body.to_owned(),
        r#ref: None,
        rc: None,
        detail: None,
    };
    append_message_to(&dir, box_name.unwrap_or("bus.ndjson"), &message)?;
    record_control_edge(&message)?;
    deliver_hail(registry, &dir, &message, socket)?;
    line(&format!(
        "to await the reply: boop wait {}   (or: boop wait --me &)",
        message.id
    ));
    let Some(timeout_secs) = wait_timeout else {
        return Ok(());
    };
    let idle = crate::cli::job::watch_turn_end(&dir, &message.id, timeout_secs);
    wait_and_exit(
        &dir,
        Watch::Reply { id: message.id },
        timeout_secs,
        None,
        mail_dir_arg,
        idle,
    )
}

/// Put one queued message in front of its recipient, through the door its
/// harness declares. Every attempt leaves one `agent_delivery` row.
pub(crate) fn deliver_hail(
    registry: &Registry,
    dir: &Path,
    message: &bus::Message,
    _socket: Option<&str>,
) -> Result<()> {
    let to = message.to.as_str();
    let routes = bus::read_routes(dir)?;
    let store = boop::Store::open(boop::Store::default_path()?)?;
    if let Some(route) = routes.get(to).filter(|route| is_acpx(route)) {
        let response = crate::cli::acpx::deliver(route, &message.body)?;
        append_acks(dir, std::slice::from_ref(message))?;
        let landing = Landing::acpx(response.trim_end().to_owned());
        landing.record(&store, &message.id, to, route.harness)?;
        if let Some(reply) = landing.reply.as_deref().filter(|text| !text.is_empty()) {
            println!("{reply}");
        }
        println!("delivered {} -> {to} (acpx queue)", message.id);
        return Ok(());
    }
    // Only a door landing names a harness, and a door landing has one; a route
    // with no harness never reaches an arm that prints this word.
    let harness_id = routes
        .get(to)
        .and_then(|route| route.harness)
        .map_or_else(|| "harness".to_owned(), |id| id.to_string());
    let landing = boop::mail::deliver_hail(registry, &store, &routes, message)?;
    info!(
        to,
        message_id = message.id,
        delivery = landing.via.as_str(),
        outcome = landing.outcome(),
        "hail delivery recorded"
    );
    match &landing.delivered {
        Delivered::Injected => {
            append_acks(dir, std::slice::from_ref(message))?;
            println!(
                "delivered {} -> {to} through the {harness_id} door",
                message.id
            );
        }
        Delivered::QueuedForTurnBoundary => match landing.via {
            Via::HookInbox => println!("queued {} -> {to} (hook inbox drains it)", message.id),
            Via::LaneSupervisor => {
                println!(
                    "queued {} -> {to} (lane supervisor delivers it)",
                    message.id
                )
            }
            _ => println!(
                "queued {} -> {to} (the {harness_id} door takes it at the next turn boundary)",
                message.id
            ),
        },
        Delivered::Unreachable(why) => {
            println!("queued {} -> {to}", message.id);
            println!("{to}: {why}: message stays queued, to_timestamp null");
        }
    }
    Ok(())
}

/// An acpx route is driven by the caller's own queue, not by a harness door.
fn is_acpx(route: &Route) -> bool {
    route.mode.as_deref() == Some("acpx")
}

/// `tell-parent`: one row from the caller to the parent its registration
/// recorded. The caller spells neither end of the edge.
pub(crate) fn run_tell_parent(
    registry: &Registry,
    kind: &str,
    body: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let identity = identity::resolve_with(registry, &routes)?;
    let (caller, route) = lane::caller_route(&identity, &routes)?;
    let pick = lane::tell_parent_target(&caller, route, &routes)?;
    let parent = pick
        .parent
        .clone()
        .context("no parent edge resolved for the caller")?;
    let body = match (body, kind) {
        (Some(body), _) => body.to_owned(),
        (None, "yield") => {
            let tree = route
                .worktree_dir
                .as_deref()
                .or(route.cwd.as_deref())
                .map(Path::new);
            lane::yield_body(&caller, tree)
        }
        (None, kind) => anyhow::bail!(
            "--body is required with --kind {kind}; only `yield` carries a default body"
        ),
    };
    let message = bus::Message {
        id: bus::mint_id(),
        from: caller.clone(),
        to: parent.clone(),
        from_timestamp: bus::now_iso(),
        to_timestamp: None,
        kind: kind.to_owned(),
        reply_to: None,
        body,
        r#ref: None,
        rc: None,
        detail: None,
    };
    append_message(&dir, &message)?;
    record_control_edge(&message)?;
    println!("{caller} -> {parent} (parent from {})", pick.source);
    deliver_hail(registry, &dir, &message, None)?;
    line(&message.id);
    Ok(())
}

/// `tell-children`: one body to every child of the caller, from the registry's
/// parent edges and from the store's `spawned` edges for the caller's session.
/// Every target reports its own outcome and the run ends in a tally, so a run
/// that reached nobody cannot read as success.
pub(crate) fn run_tell_children(
    registry: &Registry,
    body: &str,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let identity = identity::resolve_with(registry, &routes)?;
    let (caller, _) = lane::caller_route(&identity, &routes)?;
    let children = lane::children_of(&caller, &routes);
    let spawned = spawned_children(identity.session.as_deref(), &routes);
    if children.is_empty() && spawned.is_empty() {
        println!("no child of {caller} is registered");
        return Ok(());
    }
    let (mut landed, mut unreachable, mut dead) = (0usize, 0usize, 0usize);
    for (name, route) in children {
        let reach = child_reach(route, name, None);
        match &reach {
            ChildReach::NoRoute(why) => {
                unreachable += 1;
                println!("no-route {name} ({why})");
                continue;
            }
            ChildReach::Dead(target) => {
                dead += 1;
                println!("dead {name} (tmux {target} is gone)");
                continue;
            }
            _ => {}
        }
        let message = bus::Message {
            id: bus::mint_id(),
            from: caller.clone(),
            to: name.to_owned(),
            from_timestamp: bus::now_iso(),
            to_timestamp: None,
            kind: "note".to_owned(),
            reply_to: None,
            body: body.to_owned(),
            r#ref: None,
            rc: None,
            detail: None,
        };
        append_message(&dir, &message)?;
        record_control_edge(&message)?;
        match reach {
            ChildReach::Hook => {
                landed += 1;
                println!("landed {name} {} (hook inbox)", message.id);
            }
            ChildReach::Supervisor => {
                landed += 1;
                println!("landed {name} {} (lane supervisor)", message.id);
            }
            ChildReach::Pane => match deliver_through_door(registry, route, &message.body)? {
                Delivered::Injected => {
                    landed += 1;
                    println!("landed {name} {} (through the door)", message.id);
                }
                Delivered::QueuedForTurnBoundary => {
                    landed += 1;
                    println!("landed {name} {} (next turn boundary)", message.id);
                }
                Delivered::Unreachable(why) => {
                    unreachable += 1;
                    println!("no-route {name} ({why})");
                }
            },
            ChildReach::NoRoute(_) | ChildReach::Dead(_) => unreachable!("reported above"),
        }
    }
    for session in spawned {
        unreachable += 1;
        println!("no-route {session} ({NATIVE_CHILD_REASON})");
    }
    println!("{landed} landed, {unreachable} no-route, {dead} dead");
    Ok(())
}

/// One body to a child that holds a pane, through its harness's own door.
/// The pane the route names is looked up in that harness's live registry; a
/// route naming no harness, or a pane no live session holds, is unreachable
/// rather than typed at.
fn deliver_through_door(registry: &Registry, route: &Route, body: &str) -> Result<Delivered> {
    let Some(harness) = route.harness else {
        return Ok(Delivered::Unreachable("route names no harness".into()));
    };
    let adapter = registry.get(harness);
    let Some(target) = route.tmux.as_deref().filter(|target| !target.is_empty()) else {
        return Ok(Delivered::Unreachable("route names no pane".into()));
    };
    let pane = boop::live::pane_of_target(target).unwrap_or_else(|| target.to_owned());
    let Some(live) = adapter.live().live_session_in_pane(&pane)? else {
        return Ok(Delivered::Unreachable(format!(
            "no live {harness} session in {pane}"
        )));
    };
    adapter.door().deliver(&live, body)
}

/// A claude Agent-tool child runs inside its parent's process. It owns no pane,
/// no stdin and no registry route, so no delivery path in boop addresses one.
const NATIVE_CHILD_REASON: &str = "native subagent: no pane, no route, nothing drains its mailbox";

/// Children the store's `spawned` edges name under the caller's session, minus
/// the ones a registry route already carries. A store that will not open costs
/// the store-derived half of the list and nothing else.
fn spawned_children(session: Option<&str>, routes: &BTreeMap<String, Route>) -> Vec<String> {
    let Some(session) = session else {
        return Vec::new();
    };
    let rows = boop::Store::default_path()
        .and_then(boop::Store::open)
        .and_then(|store| store.edge_rows(Some(session)));
    let rows = match rows {
        Ok(rows) => rows,
        Err(error) => {
            debug!(%error, session, "spawn edges unread; registry children only");
            return Vec::new();
        }
    };
    let mut children: Vec<String> = rows
        .into_iter()
        .filter(|row| row.edge == "spawned" && row.parent == session)
        .map(|row| row.child)
        .filter(|child| {
            !routes
                .values()
                .any(|route| route.session_id.as_deref() == Some(child.as_str()))
        })
        .collect();
    children.sort();
    children.dedup();
    children
}

/// What would take a row addressed to a child.
pub(crate) enum ChildReach {
    /// An installed hook drains the child's mailbox at its turn boundary.
    Hook,
    /// The lane's own supervisor reads the mailbox.
    Supervisor,
    /// A live native session may accept harness control.
    Pane,
    /// Nothing addresses this child at all, and the reason why.
    NoRoute(&'static str),
    /// The child named a tmux target and tmux no longer has it.
    Dead(String),
}

/// How a queued row reaches a child. A route with no hook and no tmux target
/// was never reachable; a route whose target tmux has dropped went dead. The
/// two are different facts and are reported apart.
pub(crate) fn child_reach(route: &Route, name: &str, socket: Option<&str>) -> ChildReach {
    if route
        .cwd
        .as_deref()
        .is_some_and(|cwd| inbox::installed_for(Path::new(cwd), name))
    {
        return ChildReach::Hook;
    }
    let Some(target) = route.tmux.as_deref().filter(|target| !target.is_empty()) else {
        return ChildReach::NoRoute("no hook, no pane");
    };
    if !tmux::mux().target_alive(socket, target) {
        return ChildReach::Dead(target.to_owned());
    }
    match route.kind.as_str() {
        "lane" => ChildReach::Supervisor,
        _ => ChildReach::Pane,
    }
}

pub(crate) fn record_control_edge(message: &boop::bus::Message) -> Result<()> {
    if !matches!(
        message.kind.as_str(),
        "hail" | "result" | "retry" | "resume" | "cancel"
    ) {
        return Ok(());
    }
    let store = boop::Store::open(boop::Store::default_path()?)?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    store.add_edge_at(&message.from, &message.to, &message.kind, timestamp)?;
    Ok(())
}

pub(crate) fn run_inbox(cmd: InboxCmd) -> Result<()> {
    match cmd {
        InboxCmd::Drain {
            as_name,
            hook,
            mail_dir,
        } => run_inbox_drain(as_name.as_deref(), hook.into(), mail_dir.as_deref()),
        InboxCmd::Hooks {
            name,
            cwd,
            uninstall,
        } => {
            let cwd = match cwd {
                Some(cwd) => cwd,
                None => std::env::current_dir().context("read the current directory")?,
            };
            let changed = write_inbox_hooks(&cwd, &name, uninstall)?;
            report_inbox_hooks(&cwd, &name, uninstall, changed);
            Ok(())
        }
    }
}

/// Hand over every unread row addressed to `name`, once. The bus ack and the
/// drained-id ledger are both written before anything is printed: a batch this
/// process printed and then died on must never be printed twice.
pub(crate) fn run_inbox_drain(
    as_name: Option<&str>,
    hook: boop::inbox::Hook,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let name = waiting_as(&dir, as_name)?;
    let ledger = inbox::ledger_path(&dir, &name);
    let rows = inbox::undelivered(&all_messages(&dir)?, &name, &inbox::drained(&ledger));
    if rows.is_empty() {
        debug!(
            inbox = name,
            hook = hook.as_str(),
            "inbox drain found nothing"
        );
        return Ok(());
    }
    let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
    inbox::record_drained(&ledger, &ids)?;
    append_acks(&dir, &rows)?;
    info!(
        inbox = name,
        hook = hook.as_str(),
        rows = rows.len(),
        "inbox drained"
    );
    line(&hook.payload(&inbox::batch_text(
        &rows,
        &boop::supervise::mood_template(&name),
    )));
    Ok(())
}

/// Write the coordinator's hooks into its project settings, or take them out.
/// Returns how many hook entries changed; 0 means the file already said this.
pub(crate) fn write_inbox_hooks(cwd: &Path, name: &str, uninstall: bool) -> Result<usize> {
    let path = inbox::settings_path(cwd);
    if uninstall && !path.exists() {
        return Ok(0);
    }
    // The CAS closure is `Fn` and may run more than once, so the count comes
    // out through a cell rather than by assignment.
    let changed = std::cell::Cell::new(0);
    bus::cas_update_json(&path, |settings| {
        changed.set(match uninstall {
            true => inbox::uninstall(settings, name),
            false => inbox::install(settings, name),
        });
        Ok(())
    })?;
    Ok(changed.get())
}

pub(crate) fn report_inbox_hooks(cwd: &Path, name: &str, uninstall: bool, changed: usize) {
    let path = inbox::settings_path(cwd);
    let verb = match (uninstall, changed) {
        (false, 0) => "already installed for",
        (false, _) => "installed for",
        (true, 0) => "nothing to remove for",
        (true, _) => "removed for",
    };
    println!("inbox hooks {verb} {name} in {}", path.display());
    if !uninstall {
        for hook in boop::inbox::Hook::installed() {
            if let Some(event) = hook.event() {
                println!("  {event}: {}", inbox::drain_command(name, hook));
            }
        }
    }
}
