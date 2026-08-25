use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use boop::bus::Route;
use boop::door::Delivered;
use boop::harness::HarnessId;
use boop::mail::Landing;
use boop::mailwait::Watch;
use boop::registry::Registry;
use boop::{bus, identity, inbox, lane, tmux};

use crate::cli::job::waiting_as;
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
    bus::read_messages(dir)
}

/// One send, spelled once: `boop beep <route> <body>` is its one spelling.
pub(crate) struct Outbound<'a> {
    /// A registry name, or the `parent` / `children` alias.
    pub route: &'a str,
    /// `None` is legal only with `--kind yield`, which mints its own body.
    pub body: Option<&'a str>,
    pub kind: &'a str,
    /// Who the row is from, when the whoami ladder cannot say.
    pub as_name: Option<&'a str>,
    /// A mailbox other than `bus`.
    pub box_name: Option<&'a str>,
    pub timeout_secs: u64,
    /// Block for the answer. `--no-wait` clears it.
    pub wait: bool,
    pub mail_dir: Option<&'a Path>,
}

/// Route names a send cannot address: clap resolves each to the `beep`
/// subcommand of that name long before the send sees it, so a registry row
/// wearing one is unreachable and says so rather than mailing into the void.
const RESERVED_ROUTES: [&str; 8] = [
    "lane", "agent", "hail", "message", "ps", "pstree", "harness", "help",
];

/// Who a row is from when no `--as` and no ladder rung names the caller.
const DEFAULT_SENDER: &str = "coordinator";

/// Fan a body out to every child of the caller instead of addressing one.
const CHILDREN_ALIAS: &str = "children";

pub(crate) fn run_send(registry: &Registry, send: Outbound<'_>) -> Result<()> {
    if RESERVED_ROUTES.contains(&send.route) {
        anyhow::bail!(
            "`{route}` is the name of a `boop beep` subcommand, so it cannot be a route: \
             `boop beep {route} ...` runs that subcommand. Rename the route.",
            route = send.route
        );
    }
    let dir = mail_dir(send.mail_dir)?;
    let routes = bus::read_routes(&dir)?;
    if send.route == CHILDREN_ALIAS {
        return fan_out_to_children(registry, &dir, &routes, &send);
    }
    // Only the aliases that read an edge need the caller's own route; every
    // other send takes the name it was handed, registered or not.
    let (sender, to, parent_source) = if send.route == PARENT_ALIAS {
        let (caller, route, stamped) = caller_identity(registry, &routes, send.as_name)?;
        let pick = lane::tell_parent_target(&caller, route, &routes, stamped.as_deref())?;
        let parent = pick
            .parent
            .clone()
            .context("no parent edge resolved for the caller")?;
        (caller, parent, Some(pick.source))
    } else {
        (
            sender_name(registry, &routes, send.as_name)?,
            send.route.to_owned(),
            None,
        )
    };
    let body = match (send.body, send.kind) {
        (Some(body), _) => body.to_owned(),
        (None, "yield") => {
            let tree = routes
                .get(&sender)
                .and_then(|route| route.worktree_dir.as_deref().or(route.cwd.as_deref()))
                .map(Path::new);
            lane::yield_body(&sender, tree)
        }
        (None, kind) => anyhow::bail!(
            "a body is required with --kind {kind}; only `yield` carries a default body"
        ),
    };
    let message = bus::Message {
        id: bus::mint_id(),
        from: sender.clone(),
        to: to.clone(),
        from_timestamp: bus::now_iso(),
        to_timestamp: None,
        kind: send.kind.to_owned(),
        reply_to: None,
        body,
        r#ref: None,
        rc: None,
        detail: None,
    };
    append_message_to(&dir, send.box_name.unwrap_or("bus"), &message)?;
    record_control_edge(&message)?;
    if let Some(source) = parent_source {
        println!("{sender} -> {to} (parent from {source})");
    }
    deliver_hail(registry, &dir, &message, None)?;
    if parent_source.is_some() {
        print_tell_parent_receipt(&dir, &sender, &to, &message.id);
        line(&message.id);
    }
    // The parent send's last line is its own id, by contract. Every other send
    // names the command that collects the answer, before the block as well as
    // instead of it: a shell killed mid-wait leaves the id on screen.
    if parent_source.is_none() {
        line(&format!(
            "to await the reply: boop wait {}   (or: boop wait --me &)",
            message.id
        ));
    }
    if !send.wait {
        return Ok(());
    }
    push_wait(&dir, &to, &message.id, send.timeout_secs)
}

/// Who the row is from: `--as`, else the identity ladder's own name, else the
/// placeholder. A name `--as` gives is taken as written; only the alias sends
/// need it to be a registered route.
fn sender_name(
    registry: &Registry,
    routes: &BTreeMap<String, Route>,
    as_name: Option<&str>,
) -> Result<String> {
    if let Some(name) = as_name {
        return Ok(name.to_owned());
    }
    let identity = identity::resolve_with(registry, routes)?;
    Ok(lane::caller_route(&identity, routes)
        .map(|(caller, _)| caller)
        .unwrap_or_else(|_| DEFAULT_SENDER.to_owned()))
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
    let store = bus::open_store(dir)?;
    let routes = bus::routes_in(&store)?;
    if let Some(route) = routes.get(to).filter(|route| is_acpx(route)) {
        let response = crate::cli::acpx::deliver(route, &message.body)?;
        append_acks(dir, std::slice::from_ref(message))?;
        let landing = Landing::acpx(response.trim_end().to_owned());
        let harness_id = route
            .harness
            .map_or_else(|| "acpx".to_owned(), |id| id.to_string());
        if !store.has_delivery_transition(&message.id)? {
            store.append_delivery_transition(
                &message.id,
                to,
                route.harness,
                boop::DeliveryState::Appended.as_str(),
                "mailbox",
                None,
                boop::live::now_ms(),
            )?;
        }
        landing.record(&store, &message.id, to, route.harness)?;
        if let Some(reply) = landing.reply.as_deref().filter(|text| !text.is_empty()) {
            println!("{reply}");
        }
        println!("{}", landing.line(&message.id, to, &harness_id));
        return Ok(());
    }
    // The door rung is the only line that names a harness; a route naming none
    // reads the placeholder rather than inventing one.
    let harness_id = routes
        .get(to)
        .and_then(|route| route.harness)
        .map_or_else(|| "harness".to_owned(), |id| id.to_string());
    let landing = boop::mail::deliver_hail(registry, &store, &routes, message)?;
    info!(
        to,
        message_id = message.id,
        rung = landing.rung.as_str(),
        outcome = landing.outcome(),
        "hail delivery recorded"
    );
    if landing.rung.carried_the_body() {
        append_acks(dir, std::slice::from_ref(message))?;
    }
    println!("{}", landing.line(&message.id, to, &harness_id));
    confirm_transition_recorded(&store, &message.id, to)?;
    Ok(())
}

/// One POLL after the append, the ledger must hold a transition past
/// `appended` for this message. A row nobody owns is the failure the sender
/// reports, and it is the only outcome that is not an exit 0.
fn confirm_transition_recorded(store: &boop::Store, message_id: &str, to: &str) -> Result<()> {
    let deadline = std::time::Instant::now() + DELIVERY_CONFIRM;
    loop {
        let rows = store.delivery_rows(message_id).unwrap_or_default();
        if rows.iter().any(|row| {
            boop::DeliveryState::parse(&row.outcome).is_some_and(boop::DeliveryState::landed)
        }) {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            let states: Vec<&str> = rows.iter().map(|row| row.outcome.as_str()).collect();
            anyhow::bail!(
                "{message_id} -> {to}: appended with no landing inside {}ms (ledger: {}); the row is in the mailbox and nothing owns it",
                DELIVERY_CONFIRM.as_millis(),
                if states.is_empty() {
                    "empty".to_owned()
                } else {
                    states.join(", ")
                }
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// One supervisor POLL. A send with no landing by now is a row no rung of the
/// ladder took.
const DELIVERY_CONFIRM: std::time::Duration = std::time::Duration::from_millis(700);

/// The block half of a waited send. Every source it watches is one an existing verb
/// already watches: `boop wait`'s reply selection, `beep hail --wait-timeout`'s
/// turn-end receiver, and `beep lane wait`'s route liveness.
fn push_wait(dir: &Path, to: &str, message_id: &str, timeout_secs: u64) -> Result<()> {
    let watch = Watch::Reply {
        id: message_id.to_owned(),
    };
    let turn_end = crate::cli::job::watch_turn_end(dir, message_id, timeout_secs);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let next_wait = format!("boop wait {message_id}");
    let next_debug = format!("boop debug {to}");
    info!(to, message_id, timeout_secs, "push wait starting");
    let mut dead_polls = 0u32;
    loop {
        let arrivals = watch.arrivals(&all_messages(dir)?);
        if let Some(reply) = arrivals.first() {
            info!(to, message_id, "push answered by a reply");
            line(&reply.body);
            append_acks(dir, std::slice::from_ref(reply))?;
            line(&next_wait);
            return Ok(());
        }
        if let Some(ended) = turn_end.as_ref().and_then(|rx| rx.try_recv().ok()) {
            info!(to, message_id, "push answered by a turn end");
            line(&ended);
            line(&next_wait);
            return Ok(());
        }
        // A route is written before its session answers, so one dead
        // observation is never enough. The same bound `beep lane wait` uses.
        dead_polls = match crate::cli::job::route_liveness(dir, to) {
            crate::cli::job::RouteLiveness::Dead => dead_polls + 1,
            _ => 0,
        };
        if dead_polls >= crate::cli::job::DEAD_POLLS {
            warn!(to, message_id, exit_code = 3, "push route died");
            line(&format!("{to} died with no answer to {message_id}"));
            line(&next_debug);
            std::process::exit(3);
        }
        if std::time::Instant::now() >= deadline {
            info!(to, message_id, exit_code = 124, "push timed out");
            let timed_out = format!("no answer from {to} in {timeout_secs}s (id {message_id})");
            line(&timed_out);
            eprintln!("{timed_out}"); // @eprintln-ok: the next line must survive a redirected stdout
            line(&next_debug);
            eprintln!("{next_debug}"); // @eprintln-ok: same
            std::process::exit(124);
        }
        std::thread::sleep(PUSH_POLL);
    }
}

/// How often `push` re-reads the mailbox. The same cadence `boop wait` uses.
const PUSH_POLL: std::time::Duration = std::time::Duration::from_millis(500);

/// An acpx route is driven by the caller's own queue, not by a harness door.
fn is_acpx(route: &Route) -> bool {
    route.mode.as_deref() == Some("acpx")
}

/// Who is calling, for every verb that has to know: the name, its registry
/// route, and the parent the spawner stamped into the environment.
///
/// `--as` is the whole identity. A native subagent runs inside its spawner's
/// environment, so the env rung names the spawner and `BOOP_PARENT` is the
/// spawner's parent, never the native's.
fn caller_identity<'a>(
    registry: &Registry,
    routes: &'a BTreeMap<String, Route>,
    as_name: Option<&str>,
) -> Result<(String, &'a Route, Option<String>)> {
    match as_name {
        Some(name) => {
            let route = routes.get(name).with_context(|| {
                format!("--as {name} names no registered route; `beep agent register {name}` first")
            })?;
            Ok((name.to_owned(), route, None))
        }
        None => {
            let identity = identity::resolve_with(registry, routes)?;
            let (caller, route) = lane::caller_route(&identity, routes)?;
            Ok((caller, route, identity.parent))
        }
    }
}

/// The one route name that is an alias rather than a registry key.
const PARENT_ALIAS: &str = "parent";

/// The receipt a `parent` send leaves (spec 7.5): who called, which parent the
/// edge resolved to, the message id, and the transition the ladder recorded.
/// Read back from the store, so it is the same row `boop db` and `boop debug`
/// show rather than a second account of the same send.
fn print_tell_parent_receipt(dir: &Path, caller: &str, parent: &str, message_id: &str) {
    let rows = bus::open_store(dir)
        .and_then(|store| store.delivery_rows(message_id))
        .unwrap_or_default();
    match rows.last() {
        Some(row) => line(&format!(
            "receipt {caller} -> {parent} {message_id} {} ({})",
            row.outcome, row.detail
        )),
        None => line(&format!(
            "receipt {caller} -> {parent} {message_id} no landing recorded"
        )),
    }
}

/// The `children` route: one body to every child of the caller, from the
/// registry's parent edges and from the store's `spawned` edges for the
/// caller's session. Every target reports its own outcome and the run ends in
/// a tally, so a run that reached nobody cannot read as success. A fan-out has
/// no single row to wait on, so `--timeout` and `--no-wait` do not reach here.
fn fan_out_to_children(
    registry: &Registry,
    dir: &Path,
    routes: &BTreeMap<String, Route>,
    send: &Outbound<'_>,
) -> Result<()> {
    let (caller, _, _) = caller_identity(registry, routes, send.as_name)?;
    let body = send
        .body
        .context("a body is required to mail the caller's children")?;
    let identity = identity::resolve_with(registry, routes).unwrap_or_default();
    let children = lane::children_of(&caller, routes);
    let spawned = spawned_children(identity.session.as_deref(), routes);
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
        append_message(dir, &message)?;
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
