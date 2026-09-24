//! `boop beep shout` / `boop beep scream`: one row (and, for a scream, one
//! interrupt) per connected agent, through the same ladder every send walks.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::info;

use boop::bus::Route;
use boop::live::{LiveSession, LiveStatus};
use boop::registry::Registry;
use boop::{bus, identity, lane, tmux};

use crate::cli::{append_message, mail_dir};

/// The body a bare `shout` sends.
pub(crate) const SHOUT_BODY: &str = "stahp what ur doing please";
/// The body a bare `scream` sends.
pub(crate) const SCREAM_BODY: &str = "stop what ur doing check ps";
/// A broadcast whose sender resolves to no route and carries no `--as` came
/// from a laneless shell: the owner typing, never an agent.
pub(crate) const HUMAN_MARK: &str =
    "[HUMAN MESSAGE: typed by the owner from a laneless shell, not by an agent]";

/// The body one broadcast row carries: a laneless sender is marked human.
fn broadcast_body(caller: Option<&str>, body: &str) -> String {
    match caller {
        Some(_) => body.to_owned(),
        None => format!("{HUMAN_MARK} {body}"),
    }
}

/// How one route stands, measured before any row is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reach {
    /// The route's tmux target is alive; the pane id a key press takes.
    LivePane(String),
    /// A pane-less coordinator or native: addressable while registered.
    DoorOnly,
    /// Nothing live to address: a dropped pane, or a lane with no pane (a
    /// retired lane is revived by its own send, never by a broadcast).
    Dead(&'static str),
}

/// How one route stands right now. `alive` is the tmux seam
/// `(socket, target) -> live`, injected so selection needs no server.
fn reach_of(route: &Route, alive: &mut impl FnMut(Option<&str>, &str) -> bool) -> Reach {
    match route.tmux.as_deref().filter(|t| !t.is_empty()) {
        Some(target) if alive(None, target) => {
            Reach::LivePane(boop::live::pane_of_target(target).unwrap_or_else(|| target.to_owned()))
        }
        Some(_) => Reach::Dead("tmux target is gone"),
        None => match route.kind.as_str() {
            "coordinator" | "native" => Reach::DoorOnly,
            _ => Reach::Dead("no pane"),
        },
    }
}

/// Every connected route except the caller; `alive` is the tmux seam
/// `(socket, target) -> live`, injected so selection needs no server.
pub(crate) fn connected<'a>(
    routes: &'a BTreeMap<String, Route>,
    caller: Option<&str>,
    mut alive: impl FnMut(Option<&str>, &str) -> bool,
) -> Vec<(&'a str, &'a Route, Reach)> {
    routes
        .iter()
        .filter(|(name, _)| Some(name.as_str()) != caller)
        .filter_map(|(name, route)| {
            let reach = reach_of(route, &mut alive);
            matches!(reach, Reach::LivePane(_) | Reach::DoorOnly).then_some((
                name.as_str(),
                route,
                reach,
            ))
        })
        .collect()
}

/// Who the broadcast is from: an explicit `--as` is the sender even when it is
/// not a registered route (the UI sends as `instant`); without one, the whoami
/// ladder must name a registered route.
fn caller_name(routes: &BTreeMap<String, Route>, as_name: Option<&str>) -> Option<String> {
    match as_name {
        Some(name) => Some(name.to_owned()),
        None => {
            let identity = identity::resolve_as(None);
            lane::caller_route(&identity, routes)
                .map(|(caller, _)| caller)
                .ok()
        }
    }
}

/// Composer keys are authorized only for a measured busy session. A retained
/// pane, idle composer, or unknown status cannot authorize a key press.
fn interrupt_key(
    reach: &Reach,
    session: Option<&LiveSession>,
    declared_key: Option<&'static str>,
) -> std::result::Result<&'static str, &'static str> {
    let Reach::LivePane(pane) = reach else {
        return Err("no live TUI pane");
    };
    let session = session.ok_or("no live harness session")?;
    if session.tmux_pane.as_ref().is_some_and(|held| held != pane) {
        return Err("session occupies a different pane");
    }
    match session.status {
        LiveStatus::Idle => return Err("session is idle"),
        LiveStatus::Unknown => return Err("turn status is unknown"),
        LiveStatus::Busy => {}
    }
    declared_key.ok_or("harness declares no interrupt key")
}

pub(crate) struct Broadcast<'a> {
    pub targets: Option<&'a [String]>,
    pub body: &'a str,
    pub kind: &'a str,
    pub as_name: Option<&'a str>,
    /// A scream sends keys and cancel rows; a shout sends rows only.
    pub interrupt: bool,
}

/// One row (and, for a scream, one key press) per connected route, ending in
/// a tally so a broadcast that reached nobody cannot read as success.
pub(crate) fn run_broadcast(
    registry: &Registry,
    mail_dir_arg: Option<&Path>,
    broadcast: &Broadcast<'_>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let caller = caller_name(&routes, broadcast.as_name);
    let mux = tmux::mux();

    // An explicit set is a snapshot: every requested route is reported, never
    // silently widened to a broadcast and never a silent success when none of
    // them is reachable.
    let mut unreachable: Vec<String> = Vec::new();
    let mut targets: Vec<(String, &Route, Reach)> = Vec::new();
    match broadcast.targets {
        Some(names) => {
            if names.is_empty() {
                anyhow::bail!("no explicit recipients given");
            }
            let mut seen = std::collections::BTreeSet::new();
            for name in names {
                if !seen.insert(name) {
                    continue;
                }
                let Some(route) = routes.get(name.as_str()) else {
                    println!("no-route {name} (unknown route)");
                    unreachable.push(name.clone());
                    continue;
                };
                let reach = reach_of(route, &mut |socket, target| {
                    mux.target_alive(socket, target)
                });
                match reach {
                    Reach::LivePane(_) | Reach::DoorOnly => {
                        targets.push((name.clone(), route, reach));
                    }
                    Reach::Dead(why) => {
                        println!("no-route {name} ({why})");
                        unreachable.push(name.clone());
                    }
                }
            }
        }
        None => {
            targets = connected(&routes, caller.as_deref(), |socket, target| {
                mux.target_alive(socket, target)
            })
            .into_iter()
            .map(|(name, route, reach)| (name.to_owned(), route, reach))
            .collect();
        }
    }
    if targets.is_empty() {
        if unreachable.is_empty() {
            println!("no connected agent to receive a broadcast");
            return Ok(());
        }
        anyhow::bail!("no reachable recipients: {}", unreachable.join(", "));
    }
    let store = bus::open_store(&dir)?;
    let budget = boop::mail::DoorBudget::from_env();
    let (mut landed, mut cooled, mut dead) = (0usize, 0usize, 0usize);
    let mut interrupted_panes = BTreeMap::new();
    for (name, route, reach) in &targets {
        let name: &str = name;
        let route: &Route = route;
        let kind = match (broadcast.interrupt, route.kind.as_str()) {
            (true, "lane") => "cancel",
            _ => broadcast.kind,
        };
        let ready = if broadcast.interrupt && route.kind.as_str() != "lane" {
            if let Reach::LivePane(pane) = reach {
                if let Some(ready) = interrupted_panes.get(pane) {
                    println!("interrupt-skipped {name} (pane {pane} already addressed)");
                    *ready
                } else {
                    let ready = press_interrupt_keys(registry, name, route, reach)?;
                    if let Some(ready) = ready {
                        interrupted_panes.insert(pane.clone(), ready);
                    }
                    ready.unwrap_or(true)
                }
            } else {
                press_interrupt_keys(registry, name, route, reach)?.unwrap_or(true)
            }
        } else {
            true
        };
        let message = bus::Message {
            id: bus::mint_id(),
            from: caller.clone().unwrap_or_else(|| "coordinator".into()),
            to: name.to_owned(),
            from_timestamp: bus::now_iso(),
            to_timestamp: None,
            kind: kind.into(),
            reply_to: None,
            body: broadcast_body(caller.as_deref(), broadcast.body),
            r#ref: None,
            rc: None,
            detail: None,
        };
        append_message(&dir, &message)?;
        crate::cli::mail::record_control_edge(&message)?;
        if !ready {
            dead += 1;
            println!(
                "no-route {name} (interrupt not confirmed idle; message {} held in mailbox)",
                message.id
            );
            continue;
        }
        let landing = boop::mail::deliver_hail_budgeted(
            registry,
            &store,
            &routes,
            &message,
            &boop::mail::TmuxPaster,
            &budget,
        )
        .with_context(|| format!("broadcast to {name}"))?;
        let owned_inbox = matches!(
            (&reach, landing.rung),
            (Reach::DoorOnly, boop::mail::Rung::HookInbox)
        ) | matches!(landing.rung, boop::mail::Rung::TurnBoundary);
        match landing.rung {
            rung if rung.carried_the_body() || owned_inbox => {
                landed += 1;
                println!("landed {name} {} ({})", message.id, rung.as_str());
            }
            boop::mail::Rung::CoolOff => {
                cooled += 1;
                println!("cooled-off {name} {} ({})", message.id, landing.detail());
            }
            _ => {
                dead += 1;
                println!("no-route {name} ({})", landing.detail());
            }
        }
    }
    println!("{landed} landed, {cooled} cooled-off, {dead} no-route");
    info!(landed, cooled, dead, "broadcast complete");
    Ok(())
}

/// Stop a busy TUI, then wait for its idle signal before delivering the hail.
/// Lanes consume their cancel row through the supervisor instead.
fn press_interrupt_keys(
    registry: &Registry,
    name: &str,
    route: &Route,
    reach: &Reach,
) -> Result<Option<bool>> {
    if route.kind.as_str() == "lane" {
        return Ok(None);
    }
    let Some(id) = route.harness else {
        println!("interrupt-skipped {name} (route declares no harness)");
        return Ok(None);
    };
    let harness = registry.get(id);
    let session = match harness.live().live_session_for_route(route) {
        Ok(session) => session,
        Err(error) => {
            println!("interrupt-skipped {name} (live session lookup failed: {error})");
            return Ok(None);
        }
    };
    let key = match interrupt_key(
        reach,
        session.as_ref(),
        harness.capabilities().interrupt_keys,
    ) {
        Ok(key) => key,
        Err(why) => {
            println!("interrupt-skipped {name} ({why})");
            return Ok(None);
        }
    };
    let Reach::LivePane(pane) = reach else {
        unreachable!()
    };
    let session = session.expect("interrupt_key verified the live session");
    crate::cli::paste::send_keys(pane, &[key], false)
        .with_context(|| format!("interrupt {name} in pane {pane}"))?;
    println!("interrupt-sent {name} in {pane} with {key}");
    match harness.door().notify_idle(&session, Duration::from_secs(2)) {
        Ok(_) => {
            println!("interrupted {name} in {pane} (idle confirmed)");
            Ok(Some(true))
        }
        Err(error) => {
            println!("interrupt-unconfirmed {name} ({error})");
            Ok(Some(false))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boop::harness::HarnessId;
    use std::collections::BTreeSet;

    fn route(kind: &str, harness: Option<HarnessId>, tmux: Option<&str>) -> Route {
        Route {
            kind: kind.into(),
            harness,
            tmux: tmux.map(str::to_owned),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        }
    }

    fn registry_of(rows: &[(&str, &str, Option<&str>)]) -> BTreeMap<String, Route> {
        rows.iter()
            .map(|(name, kind, tmux)| {
                (
                    name.to_string(),
                    route(kind, Some(HarnessId::Claude), *tmux),
                )
            })
            .collect()
    }

    /// RECEIPT. Live panes and pane-less coordinators, never the caller or a
    /// dead pane; sabotage: dropping the caller filter shouts at the shouter.
    #[test]
    fn connected_routes_are_live_panes_and_paneless_coordinators() {
        let routes = registry_of(&[
            ("caller", "coordinator", Some("sess:0.0")),
            ("lane-live", "lane", Some("sess:0.1")),
            ("lane-dead", "lane", Some("gone:0.0")),
            ("lane-retired", "lane", None),
            ("coord-paneless", "coordinator", None),
            ("native", "native", None),
        ]);
        let live: BTreeSet<&str> = ["sess:0.0", "sess:0.1"].into_iter().collect();
        let picked = connected(&routes, Some("caller"), |_, target| live.contains(target));
        let names: Vec<&str> = picked.iter().map(|(name, _, _)| *name).collect();
        assert_eq!(
            names,
            ["coord-paneless", "lane-live", "native"],
            "{names:?}"
        );
        let reaches: Vec<&Reach> = picked.iter().map(|(_, _, reach)| reach).collect();
        assert_eq!(reaches[1], &Reach::LivePane("sess:0.1".into()));
    }

    /// RECEIPT. The fallback spelling exists and stays the stop-gap phrase.
    #[test]
    fn default_bodies_are_the_stop_gap_phrases() {
        assert_eq!(SHOUT_BODY, "stahp what ur doing please");
        assert_eq!(SCREAM_BODY, "stop what ur doing check ps");
    }

    /// RECEIPT. Only a laneless sender is marked human; sabotage: marking every
    /// body lets an agent's broadcast read as the owner's.
    #[test]
    fn laneless_broadcasts_are_marked_human() {
        assert_eq!(
            [
                broadcast_body(None, SCREAM_BODY),
                broadcast_body(Some("root"), SCREAM_BODY),
            ],
            [
                "[HUMAN MESSAGE: typed by the owner from a laneless shell, not by an agent] stop what ur doing check ps".to_owned(),
                "stop what ur doing check ps".to_owned(),
            ]
        );
    }

    /// The tmux seam stays a closure, so selection needs no server.
    #[test]
    fn paneless_lanes_are_dead_and_never_revived() {
        let routes = registry_of(&[("retired", "lane", None)]);
        assert!(connected(&routes, None, |_, _| true).is_empty());
    }

    #[test]
    fn caller_name_resolves_registered_senders_and_honors_explicit_as() {
        let routes = registry_of(&[("root", "coordinator", Some("sess:0.0"))]);
        assert_eq!(caller_name(&routes, Some("root")).as_deref(), Some("root"));
        assert_eq!(
            caller_name(&routes, Some("ghost")).as_deref(),
            Some("ghost"),
            "an explicit --as is the sender even when unregistered"
        );
    }

    #[test]
    fn retained_tmux_pane_without_live_harness_session_cannot_be_interrupted() {
        assert_eq!(
            interrupt_key(&Reach::LivePane("%1".into()), None, Some("Escape")),
            Err("no live harness session")
        );
    }

    #[test]
    fn interrupt_keys_require_busy_status_and_use_the_adapter_key() {
        use boop::live::{DoorAddress, LiveSessionScope};
        let registry = Registry::discover();
        let pane = Reach::LivePane("%1".into());
        let mut results = Vec::new();
        for id in [
            HarnessId::Claude,
            HarnessId::Codex,
            HarnessId::Opencode,
            HarnessId::Omp,
            HarnessId::Kimi,
        ] {
            for status in [LiveStatus::Busy, LiveStatus::Idle, LiveStatus::Unknown] {
                let session = LiveSession {
                    harness: id,
                    session_id: "test-session".into(),
                    pid: None,
                    cwd: None,
                    tmux_pane: Some("%1".into()),
                    status,
                    door: DoorAddress::None,
                    observed_ms: 0,
                    started_ms: None,
                    scope: LiveSessionScope::Root,
                    parent_session: None,
                };
                results.push(interrupt_key(
                    &pane,
                    Some(&session),
                    registry.get(id).capabilities().interrupt_keys,
                ));
            }
        }
        assert_eq!(
            results,
            vec![
                Ok("Escape"),
                Err("session is idle"),
                Err("turn status is unknown"),
                Ok("Escape"),
                Err("session is idle"),
                Err("turn status is unknown"),
                Ok("C-g"),
                Err("session is idle"),
                Err("turn status is unknown"),
                Ok("Escape"),
                Err("session is idle"),
                Err("turn status is unknown"),
                Err("harness declares no interrupt key"),
                Err("session is idle"),
                Err("turn status is unknown"),
            ]
        );
    }
}
