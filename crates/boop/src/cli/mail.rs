use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use tracing::debug;

use boop::bus::Route;
use boop::mailwait::Watch;
use boop::registry::Registry;
use boop::{bus, identity, lane, tmux};

use crate::cli::job::{harness_by_id, wait_and_exit, waiting_as};
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
            let harnesses = Registry::discover();
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
                let address = route
                    .address
                    .as_ref()
                    .map(|value| format!("{value:?}"))
                    .unwrap_or_else(|| "-".into());
                let capabilities = route
                    .harness
                    .as_deref()
                    .and_then(|id| harnesses.by_id(id))
                    .map(|harness| format!("{:?}", harness.control_capabilities()))
                    .unwrap_or_else(|| "-".into());
                line(&format!("  address={address} capabilities={capabilities}"));
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
    _socket: Option<&str>,
) -> Result<()> {
    let to = message.to.as_str();
    let routes = bus::read_routes(dir)?;
    let Some(route) = routes.get(to) else {
        println!("queued {} -> {to}", message.id);
        println!("no registry route for {to}: message stays queued, to_timestamp null");
        return Ok(());
    };
    // A lane supervisor reads this mailbox directly.
    if route.kind == "lane" {
        println!(
            "queued {} -> {to} (lane supervisor delivers it)",
            message.id
        );
        return Ok(());
    }
    if route.address.is_some() || route.session_id.is_some() {
        let envelope = dispatch_envelope(message);
        match send_address_route(registry, dir, route, &envelope)? {
            boop::harness::DeliveryReceipt::Accepted
            | boop::harness::DeliveryReceipt::Delivered
            | boop::harness::DeliveryReceipt::ParentMediated { .. } => {
                append_acks(dir, std::slice::from_ref(message))?;
                println!("delivered {} -> {to}", message.id);
            }
            boop::harness::DeliveryReceipt::Queued { reason } => {
                println!("queued {} -> {to} ({reason:?})", message.id);
            }
            boop::harness::DeliveryReceipt::Unsupported { capability } => {
                println!("queued {} -> {to} (unsupported {capability})", message.id);
            }
        }
        return Ok(());
    }
    println!("queued {} -> {to}", message.id);
    println!("{to} has no native or supervisor transport: message stays queued");
    Ok(())
}

fn dispatch_envelope(message: &bus::Message) -> String {
    format!("[boop-message-id:{}]\n{}", message.id, message.body)
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
    let (mut landed, mut unreachable) = (0usize, 0usize);
    for (name, route) in children {
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
        match send_address_route(registry, &dir, route, &dispatch_envelope(&message))? {
            boop::harness::DeliveryReceipt::Accepted
            | boop::harness::DeliveryReceipt::Delivered
            | boop::harness::DeliveryReceipt::ParentMediated { .. } => {
                landed += 1;
                println!("landed {name} {} (harness control)", message.id);
            }
            boop::harness::DeliveryReceipt::Queued { .. }
            | boop::harness::DeliveryReceipt::Unsupported { .. } => {
                unreachable += 1;
                println!("no-route {name} (harness control unavailable)");
            }
        }
    }
    for session in spawned {
        unreachable += 1;
        println!("no-route {session} (child has no registered address)");
    }
    println!("{landed} landed, {unreachable} no-route");
    Ok(())
}

fn send_address_route(
    registry: &Registry,
    dir: &Path,
    route: &Route,
    body: &str,
) -> Result<boop::harness::DeliveryReceipt> {
    let adapter = harness_by_id(registry, route.harness.as_deref().unwrap_or(""))?;
    let address = route.address.clone().or_else(|| {
        route.session_id.as_ref().map(|value| {
            boop::harness::AgentAddress::Session(boop::harness::HarnessSessionId {
                harness: route.harness.clone().unwrap_or_default(),
                value: value.clone(),
            })
        })
    });
    let Some(address) = address else {
        return Ok(boop::harness::DeliveryReceipt::Queued {
            reason: boop::harness::QueueReason::MissingSession,
        });
    };
    let sessions = boop::address::read_sessions(dir)?;
    match address {
        boop::harness::AgentAddress::Session(id) => {
            let Some(known) = sessions.get(&id).cloned() else {
                return Ok(boop::harness::DeliveryReceipt::Queued {
                    reason: boop::harness::QueueReason::MissingSession,
                });
            };
            let session = if known.control.is_some() {
                known
            } else {
                adapter.refresh_session(&known, None)?.unwrap_or(known)
            };
            let receipt = adapter.send_session(&session, body)?;
            if !matches!(
                receipt,
                boop::harness::DeliveryReceipt::Queued { .. }
                    | boop::harness::DeliveryReceipt::Unsupported { .. }
            ) {
                boop::address::upsert_session(dir, session)?;
            }
            Ok(receipt)
        }
        boop::harness::AgentAddress::Child(child) => {
            if child.parent.harness != adapter.id() || !sessions.contains_key(&child.parent) {
                return Ok(boop::harness::DeliveryReceipt::Queued {
                    reason: boop::harness::QueueReason::MissingSession,
                });
            }
            let receipt = adapter.send_child(&child, body)?;
            Ok(receipt)
        }
        boop::harness::AgentAddress::Lane { .. } => Ok(boop::harness::DeliveryReceipt::Queued {
            reason: boop::harness::QueueReason::MissingControlEndpoint,
        }),
    }
}

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

/// Legacy command retained as a diagnostic. Prompt hooks do not consume Boop
/// mail; native control or a lane supervisor owns delivery.
pub(crate) fn run_inbox_drain(
    as_name: Option<&str>,
    hook: boop::inbox::Hook,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let name = waiting_as(&dir, as_name)?;
    let rows: Vec<_> = bus::unacked(&all_messages(&dir)?)
        .into_iter()
        .filter(|row| row.to == name)
        .collect();
    if rows.is_empty() {
        debug!(inbox = name, "inbox drain found nothing");
        return Ok(());
    }
    let _ = hook;
    println!(
        "{} queued row(s) for {name}; prompt hooks are disabled",
        rows.len()
    );
    Ok(())
}

/// Write the coordinator's hooks into its project settings, or take them out.
/// Returns how many hook entries changed; 0 means the file already said this.
pub(crate) fn write_inbox_hooks(cwd: &Path, name: &str, uninstall: bool) -> Result<usize> {
    let _ = (cwd, name, uninstall);
    Ok(0)
}

pub(crate) fn report_inbox_hooks(cwd: &Path, name: &str, uninstall: bool, changed: usize) {
    let _ = (cwd, name, uninstall, changed);
    println!("prompt hooks are disabled; use native harness control or queued mail");
}
