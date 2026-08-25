//! Where one hail lands: the route's harness, its own live-session registry,
//! and the door its declared mail capability names. One ledger row per try.

use std::collections::BTreeMap;

use anyhow::Result;

use boop_harness::door::Delivered;
use boop_harness::harness::{Harness, MailPolicy};
use boop_harness::live::{pane_of_target, DoorAddress, LiveSession, LiveStatus};
use boop_harness::Registry;
use boop_store::bus::{Message, Route};
use boop_store::harness_id::HarnessId;
use boop_store::ident::{LiveRow, Store};

/// What took the text, once something did.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Via {
    /// The harness's own control plane.
    Door,
    /// A queued row an installed hook drains at the session's turn boundary.
    HookInbox,
    /// The lane's own supervisor, which reads the mailbox directly.
    LaneSupervisor,
    /// The acpx queue, driven by the caller.
    Acpx,
    /// Nothing addressed the recipient at all.
    Nothing,
}

impl Via {
    pub fn as_str(self) -> &'static str {
        match self {
            Via::Door => "door",
            Via::HookInbox => "hook inbox",
            Via::LaneSupervisor => "lane supervisor",
            Via::Acpx => "acpx queue",
            Via::Nothing => "nothing",
        }
    }
}

/// One delivery attempt: the door's answer and the transport that carried it.
pub struct Landing {
    pub delivered: Delivered,
    pub via: Via,
    /// Text the transport answered with. Only the acpx queue replies inline.
    pub reply: Option<String>,
}

impl Landing {
    pub fn door(delivered: Delivered) -> Landing {
        Landing {
            delivered,
            via: Via::Door,
            reply: None,
        }
    }

    pub fn unreachable(why: impl Into<String>) -> Landing {
        Landing {
            delivered: Delivered::Unreachable(why.into()),
            via: Via::Nothing,
            reply: None,
        }
    }

    pub fn hook_inbox() -> Landing {
        Landing {
            delivered: Delivered::QueuedForTurnBoundary,
            via: Via::HookInbox,
            reply: None,
        }
    }

    pub fn lane_supervisor() -> Landing {
        Landing {
            delivered: Delivered::QueuedForTurnBoundary,
            via: Via::LaneSupervisor,
            reply: None,
        }
    }

    pub fn acpx(reply: String) -> Landing {
        Landing {
            delivered: Delivered::Injected,
            via: Via::Acpx,
            reply: Some(reply),
        }
    }

    /// The terminal receipt state this transport reports.
    pub fn outcome(&self) -> &'static str {
        match self.delivered {
            Delivered::Injected | Delivered::QueuedForTurnBoundary => "accepted-by-harness",
            Delivered::Unreachable(_) => "rejected-by-harness",
        }
    }

    /// The ledger's `detail`: the transport that landed it, or why none did.
    pub fn detail(&self) -> String {
        match &self.delivered {
            Delivered::Unreachable(why) => why.clone(),
            _ => self.via.as_str().to_owned(),
        }
    }

    /// Append this transport's terminal transition to the delivery receipt.
    pub fn record(
        &self,
        store: &Store,
        message_id: &str,
        route: &str,
        harness: Option<HarnessId>,
    ) -> Result<()> {
        store.record_delivery(
            message_id,
            route,
            harness,
            self.outcome(),
            &self.detail(),
            boop_harness::live::now_ms(),
        )
    }
}

/// Put one queued message in front of its recipient and record what happened.
/// A lane row is left where it lies for the lane's own supervisor to read.
pub fn deliver_hail(
    registry: &Registry,
    store: &Store,
    routes: &BTreeMap<String, Route>,
    message: &Message,
) -> Result<Landing> {
    let route = routes.get(message.to.as_str());
    let harness = route.and_then(|route| route.harness);
    store.record_delivery(
        &message.id,
        &message.to,
        harness,
        "appended",
        "mailbox",
        boop_harness::live::now_ms(),
    )?;
    let lane_supervisor = route.is_some_and(|route| route.kind == "lane");
    if !lane_supervisor {
        store.record_delivery(
            &message.id,
            &message.to,
            harness,
            "submitted-to-harness",
            "delivery transport",
            boop_harness::live::now_ms(),
        )?;
    }
    let landing = land(registry, store, routes, message)?;
    if !lane_supervisor {
        landing.record(store, &message.id, &message.to, harness)?;
    }
    Ok(landing)
}

fn land(
    registry: &Registry,
    store: &Store,
    routes: &BTreeMap<String, Route>,
    message: &Message,
) -> Result<Landing> {
    let to = message.to.as_str();
    let Some(route) = routes.get(to) else {
        return Ok(Landing::unreachable(format!("no registry route for {to}")));
    };
    if route.kind == "lane" {
        return Ok(Landing::lane_supervisor());
    }
    let Some(id) = route.harness else {
        return Ok(Landing::unreachable(format!("route {to} names no harness")));
    };
    let harness = registry.get(id);
    match harness.capabilities().mail {
        MailPolicy::Keystrokes => Ok(Landing::unreachable("keystroke delivery retired")),
        MailPolicy::Door => {
            let Some(live) = live_session(harness, store, route, id)? else {
                // A door that answers nothing does not lose the row where the
                // recipient still has a hook installed to drain it.
                return Ok(match hook_landing(route, to) {
                    landing if landing.via == Via::HookInbox => landing,
                    _ => Landing::unreachable(format!("no live {id} session for {to}")),
                });
            };
            let (kind, addr) = door_columns(&live.door);
            store.record_live_door(&live.session_id, kind, addr.as_deref())?;
            Ok(Landing::door(harness.door().deliver(&live, &message.body)?))
        }
    }
}

/// A hook-backed session consumes the queued row at its turn boundary, but
/// only where its project settings actually carry the hook.
fn hook_landing(route: &Route, to: &str) -> Landing {
    match route.cwd.as_deref() {
        Some(cwd) if boop_proc::inbox::installed_for(std::path::Path::new(cwd), to) => {
            Landing::hook_inbox()
        }
        _ => Landing::unreachable(format!("no hook inbox installed for {to}")),
    }
}

/// The running session a route addresses: the harness's own registry first,
/// then the last `agent_live` projection for the session the route names.
/// The running session a route addresses; `deliver_hail` and `boop wait` share it.
pub fn live_session(
    harness: &dyn Harness,
    store: &Store,
    route: &Route,
    id: HarnessId,
) -> Result<Option<LiveSession>> {
    if let Some(target) = route.tmux.as_deref().filter(|target| !target.is_empty()) {
        let pane = pane_of_target(target).unwrap_or_else(|| target.to_owned());
        if let Some(live) = harness.live().live_session_in_pane(&pane)? {
            return Ok(Some(live));
        }
    }
    let Some(session_id) = route.session_id.as_deref() else {
        return Ok(None);
    };
    // Codex and opencode registries record no pane, so the route's session
    // id is the match; the registry carries the door the store cannot.
    if let Some(live) = harness
        .live()
        .live_sessions()?
        .into_iter()
        .find(|session| session.session_id == session_id)
    {
        return Ok(Some(live));
    }
    Ok(store.live_row(session_id)?.map(|row| projected(id, row)))
}

/// The last projection of one session read back as a live session. The status
/// text is the store's, so an unrecognised word reads as `Unknown`.
fn projected(id: HarnessId, row: LiveRow) -> LiveSession {
    LiveSession {
        harness: id,
        session_id: row.session,
        pid: row.pid.map(|pid| pid as u32),
        cwd: None,
        tmux_pane: row.tmux_pane,
        status: match row.status.as_deref() {
            Some("live") | Some("busy") => LiveStatus::Busy,
            Some("idle") => LiveStatus::Idle,
            _ => LiveStatus::Unknown,
        },
        door: door_address(row.door_kind.as_deref(), row.door_addr.as_deref()),
        observed_ms: boop_harness::live::now_ms(),
        started_ms: None,
    }
}

/// A door address as the two `agent_live` columns spell it. The claude socket
/// token is a per-process secret and is never projected into the store.
pub fn door_columns(door: &DoorAddress) -> (&'static str, Option<String>) {
    match door {
        DoorAddress::UnixSocket { path, .. } => ("unix-socket", Some(path.display().to_string())),
        DoorAddress::AppServer { socket, thread } => {
            ("app-server", Some(format!("{}#{thread}", socket.display())))
        }
        DoorAddress::Http { base, session } => ("http", Some(format!("{base}#{session}"))),
        DoorAddress::None => ("none", None),
    }
}

/// The inverse of `door_columns`. Text that names no door, or an http address
/// that no longer parses, reads as `None` rather than as a guess.
pub fn door_address(kind: Option<&str>, addr: Option<&str>) -> DoorAddress {
    let (Some(kind), Some(addr)) = (kind, addr) else {
        return DoorAddress::None;
    };
    match kind {
        "unix-socket" => DoorAddress::UnixSocket {
            path: addr.into(),
            token: None,
        },
        "app-server" => match addr.rsplit_once('#') {
            Some((socket, thread)) => DoorAddress::AppServer {
                socket: socket.into(),
                thread: thread.to_owned(),
            },
            None => DoorAddress::None,
        },
        "http" => match addr.rsplit_once('#') {
            Some((base, session)) => match url::Url::parse(base) {
                Ok(base) => DoorAddress::Http {
                    base,
                    session: session.to_owned(),
                },
                Err(_) => DoorAddress::None,
            },
            None => DoorAddress::None,
        },
        _ => DoorAddress::None,
    }
}

#[cfg(test)]
mod tests {
    use super::{door_address, door_columns};
    use boop_harness::live::DoorAddress;

    /// RECEIPT. Every door address round-trips through the two `agent_live`
    /// columns, so a store fallback addresses the same door the registry did.
    #[test]
    fn every_door_address_round_trips_through_its_columns() {
        let doors = [
            DoorAddress::UnixSocket {
                path: "/tmp/claude-42.sock".into(),
                token: None,
            },
            DoorAddress::AppServer {
                socket: "/tmp/codex.sock".into(),
                thread: "thread-9".into(),
            },
            DoorAddress::Http {
                base: url::Url::parse("http://127.0.0.1:4096/").unwrap(),
                session: "ses_1".into(),
            },
            DoorAddress::None,
        ];
        for door in doors {
            let (kind, addr) = door_columns(&door);
            assert_eq!(door_address(Some(kind), addr.as_deref()), door);
        }
        assert_eq!(
            door_address(Some("nothing-known"), Some("x")),
            DoorAddress::None
        );
        assert_eq!(door_address(None, None), DoorAddress::None);
    }
}
