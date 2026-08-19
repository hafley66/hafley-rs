use std::path::Path;

use anyhow::{Context, Result};
use tracing::{debug, info};

use boop::bus::Route;
use boop::mailwait::Watch;
use boop::registry::Registry;
use boop::{bus, identity, inbox, lane, tmux};

use crate::cli::{append_acks, append_message, append_message_to, line, mail_dir, pad};
use crate::{harness_by_id, wait_and_exit, waiting_as, InboxCmd};

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
                let padded_harness = pad(route.harness.as_deref().unwrap_or("-"), 10);
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
    wait_and_exit(
        &dir,
        Watch::Reply { id: message.id },
        timeout_secs,
        None,
        mail_dir_arg,
    )
}

/// Put one queued message in front of its recipient, by whatever its route
/// kind allows. A lane's own supervisor reads the mailbox, so a lane row is
/// left where it lies.
pub(crate) fn deliver_hail(
    registry: &Registry,
    dir: &Path,
    message: &bus::Message,
    socket: Option<&str>,
) -> Result<()> {
    let to = message.to.as_str();
    let routes = bus::read_routes(dir)?;
    let Some(route) = routes.get(to) else {
        println!("queued {} -> {to}", message.id);
        println!("no registry route for {to}: message stays queued, to_timestamp null");
        return Ok(());
    };
    // A lane pane runs the supervisor, which reads this mailbox directly;
    // typing at its stdout would reach no agent.
    if route.kind == "lane" {
        println!(
            "queued {} -> {to} (lane supervisor delivers it)",
            message.id
        );
        return Ok(());
    }
    // A session that drains its own mail at a turn boundary must never be typed
    // at: the keystrokes would land mid-turn, mid-dialog, or in a tool call. The
    // installed hook is the routing decision, so removing it restores injection.
    if let Some(cwd) = route.cwd.as_deref() {
        if inbox::installed_for(Path::new(cwd), to) {
            println!("queued {} -> {to} (hook inbox drains it)", message.id);
            info!(
                to,
                message_id = message.id,
                delivery = "hook",
                "hail queued for a hook inbox"
            );
            return Ok(());
        }
    }
    let pane = route.tmux.as_deref();
    let no_pane =
        pane.is_none() || pane.is_some_and(|target| !tmux::mux().target_alive(socket, target));
    if no_pane && matches!(route.kind.as_str(), "coordinator" | "native") {
        println!("queued {} -> {to} (no pane)", message.id);
        return Ok(());
    }
    let Some(pane) = pane else {
        println!("queued {} -> {to}", message.id);
        println!("{to} has no tmux pane: message stays queued, to_timestamp null");
        return Ok(());
    };
    match inject_mail(registry, route, message, pane, socket)? {
        boop::harness::SendOutcome::Injected => println!("injected into tmux {pane}"),
        boop::harness::SendOutcome::QueuedForNextSpawn => {
            println!("queued for next spawn -> {to}");
        }
        boop::harness::SendOutcome::Unsupported => {
            println!("{to} harness has no send support: message stays queued");
        }
    }
    Ok(())
}

/// Type one queued row into a live pane, rendered through the receiver's mood.
/// The send goes through the harness control facet; tmux is a transport detail
/// inside the impl, and the session carries the pane handle spawn gave it.
pub(crate) fn inject_mail(
    registry: &Registry,
    route: &Route,
    message: &bus::Message,
    pane: &str,
    socket: Option<&str>,
) -> Result<boop::harness::SendOutcome> {
    let to = message.to.as_str();
    let rendered = boop::supervise::render_mail(
        &boop::supervise::mood_template(to),
        &message.kind,
        &message.id,
        &message.from,
        &message.body,
    );
    let harness_id = route.harness.as_deref().unwrap_or("claude");
    let adapter = harness_by_id(registry, harness_id)?;
    let session = boop::harness::SessionRef {
        harness: adapter.id(),
        session_id: to.to_owned(),
        nickname: to.to_owned(),
        path: std::path::PathBuf::from("/tmp/hail.jsonl"),
        cwd: route.cwd.clone(),
        git_branch: None,
        modified_ms: 0,
        size: 0,
        tmux: Some(pane.to_owned()),
        tmux_socket: socket.map(str::to_owned),
        parent: None,
    };
    adapter.send(&session, &rendered)
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

/// `tell-children`: one body to every child the registry records under the
/// caller. A child nothing drains is reported dead and gets no row.
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
    if children.is_empty() {
        println!("no child of {caller} is registered");
        return Ok(());
    }
    for (name, route) in children {
        let Some(reach) = child_reach(route, name, None) else {
            println!("dead {name}");
            continue;
        };
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
            ChildReach::Hook => println!("landed {name} {} (hook inbox)", message.id),
            ChildReach::Supervisor => {
                println!("landed {name} {} (lane supervisor)", message.id)
            }
            ChildReach::Pane(pane) => match inject_mail(registry, route, &message, &pane, None)? {
                boop::harness::SendOutcome::Injected => {
                    println!("landed {name} {} (pane {pane})", message.id)
                }
                boop::harness::SendOutcome::QueuedForNextSpawn => {
                    println!("landed {name} {} (next spawn)", message.id)
                }
                boop::harness::SendOutcome::Unsupported => {
                    println!("dead {name} (harness takes no send)")
                }
            },
        }
    }
    Ok(())
}

/// What would take a row addressed to a child.
pub(crate) enum ChildReach {
    /// An installed hook drains the child's mailbox at its turn boundary.
    Hook,
    /// The lane's own supervisor reads the mailbox.
    Supervisor,
    /// A live pane takes the keystrokes.
    Pane(String),
}

/// How a queued row reaches a child, or `None` when nothing would take it: no
/// hook drains its project and its pane is gone.
pub(crate) fn child_reach(route: &Route, name: &str, socket: Option<&str>) -> Option<ChildReach> {
    if route
        .cwd
        .as_deref()
        .is_some_and(|cwd| inbox::installed_for(Path::new(cwd), name))
    {
        return Some(ChildReach::Hook);
    }
    let target = route.tmux.as_deref().filter(|target| !target.is_empty())?;
    if !tmux::mux().target_alive(socket, target) {
        return None;
    }
    match route.kind.as_str() {
        "lane" => Some(ChildReach::Supervisor),
        _ => Some(ChildReach::Pane(target.to_owned())),
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
