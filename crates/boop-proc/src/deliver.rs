//! The delivery ladder: where one hail lands, in order, and the transition
//! each rung records. Every send path in boop walks this one function.

use std::collections::BTreeMap;

use anyhow::Result;

use boop_harness::door::Delivered;
use boop_harness::harness::{Harness, MailPolicy};
use boop_harness::live::{pane_of_target, DoorAddress, LiveSession, LiveStatus};
use boop_harness::Registry;
use boop_store::bus::{Message, Route};
use boop_store::harness_id::HarnessId;
use boop_store::ident::{DeliveryState, LiveRow, Store};

/// One rung of the delivery ladder. Every send path walks these top to
/// bottom and stops at the first that takes the row, so a message is never
/// reported lost: the last rung is the mailbox itself.
///
/// | rung | condition | transition recorded |
/// |---|---|---|
/// | `Door` | a live door session takes the text into the running turn | accepted-by-harness |
/// | `Acpx` | the caller drives the recipient's own acpx queue | accepted-by-harness |
/// | `TurnBoundary` | the recipient's supervisor holds it, or a door harness whose door answered nothing holds it for its next turn | held-for-turn-boundary |
/// | `HookInbox` | the recipient's project carries an installed inbox hook | queued-in-hook-inbox |
/// | `PanePaste` | the route owns no door at all and names a live pane | pasted-into-pane |
/// | `Mailbox` | nothing answered; the row waits and the supervisor retries it | held-in-mailbox |
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Rung {
    Door,
    Acpx,
    TurnBoundary,
    HookInbox,
    PanePaste,
    Mailbox,
}

impl Rung {
    /// The transition this rung records. One rung, one state.
    pub fn state(self) -> DeliveryState {
        match self {
            Rung::Door | Rung::Acpx => DeliveryState::AcceptedByHarness,
            Rung::TurnBoundary => DeliveryState::HeldForTurnBoundary,
            Rung::HookInbox => DeliveryState::QueuedInHookInbox,
            Rung::PanePaste => DeliveryState::PastedIntoPane,
            Rung::Mailbox => DeliveryState::HeldInMailbox,
        }
    }

    /// The word the sender prints, and the same word `boop debug` shows.
    pub fn as_str(self) -> &'static str {
        match self {
            Rung::Door => "door",
            Rung::Acpx => "acpx queue",
            Rung::TurnBoundary => "turn boundary",
            Rung::HookInbox => "hook inbox",
            Rung::PanePaste => "pane paste",
            Rung::Mailbox => "mailbox",
        }
    }

    /// Whether this rung put the message body itself in front of the
    /// recipient. The paste rung leaves a notice, never the body, so it does
    /// not ack the row: the recipient still drains it.
    pub fn carried_the_body(self) -> bool {
        matches!(self, Rung::Door | Rung::Acpx)
    }
}

/// Where one message landed and why that rung. `detail` names the transport or
/// the check that sent the ladder one rung lower.
pub struct Landing {
    pub rung: Rung,
    pub detail: String,
    /// Text the transport answered with. Only the acpx queue replies inline.
    pub reply: Option<String>,
}

impl Landing {
    pub fn new(rung: Rung, detail: impl Into<String>) -> Landing {
        Landing {
            rung,
            detail: detail.into(),
            reply: None,
        }
    }

    pub fn acpx(reply: String) -> Landing {
        Landing {
            rung: Rung::Acpx,
            detail: "acpx queue".to_owned(),
            reply: Some(reply),
        }
    }

    /// The transition this landing records.
    pub fn state(&self) -> DeliveryState {
        self.rung.state()
    }

    /// The ledger's `outcome` word.
    pub fn outcome(&self) -> &'static str {
        self.state().as_str()
    }

    /// The ledger's `detail`: the transport that took it, or the check that
    /// pushed the ladder down a rung.
    pub fn detail(&self) -> String {
        self.detail.clone()
    }

    /// The one line a send verb prints: which rung took it, for whom, and the
    /// message id the reply will name. `harness` names the door when one
    /// answered and reads `harness` for a route that names none.
    pub fn line(&self, message_id: &str, to: &str, harness: &str) -> String {
        match self.rung {
            Rung::Door => format!("delivered {message_id} -> {to} through the {harness} door"),
            Rung::Acpx => format!("delivered {message_id} -> {to} through the acpx queue"),
            Rung::TurnBoundary => format!(
                "held {message_id} -> {to} for the next turn boundary ({})",
                self.detail
            ),
            Rung::HookInbox => format!(
                "queued {message_id} -> {to} in the installed inbox hook ({})",
                self.detail
            ),
            Rung::PanePaste => format!(
                "pasted {message_id} -> {to} into its pane ({})",
                self.detail
            ),
            Rung::Mailbox => format!(
                "held {message_id} -> {to} in the mailbox ({}); the supervisor retries it",
                self.detail
            ),
        }
    }

    /// Append this landing's transition to the delivery ledger.
    pub fn record(
        &self,
        store: &Store,
        message_id: &str,
        route: &str,
        harness: Option<HarnessId>,
    ) -> Result<()> {
        store.append_delivery_transition(
            message_id,
            route,
            harness,
            self.outcome(),
            &self.detail(),
            None,
            boop_harness::live::now_ms(),
        )
    }
}

/// Rung 4's seam. The tmux implementation pastes into a live pane; a caller
/// that must not touch a real terminal passes its own.
pub trait PanePaster {
    /// Paste one notice into `pane`. `Some(pane)` means the pane took it.
    fn paste(&self, pane: &str, notice: &str) -> Option<String>;
}

/// The paster every send path uses: one `tmux send-keys -l` into a live pane,
/// with no Enter. A human reads the line and a TUI prompt holds it, so nothing
/// is submitted on the recipient's behalf.
pub struct TmuxPaster;

impl PanePaster for TmuxPaster {
    fn paste(&self, pane: &str, notice: &str) -> Option<String> {
        let status = std::process::Command::new("tmux")
            .args(["send-keys", "-t", pane, "-l", notice])
            .status()
            .ok()?;
        status.success().then(|| pane.to_owned())
    }
}

/// Put one queued message in front of its recipient and record every step.
/// Two transitions at minimum: `appended` when the row exists, then the rung
/// the ladder stopped on. A sender that sees no second row has a store it
/// cannot write, which is the one condition that fails a send.
pub fn deliver_hail(
    registry: &Registry,
    store: &Store,
    routes: &BTreeMap<String, Route>,
    message: &Message,
) -> Result<Landing> {
    deliver_hail_with(registry, store, routes, message, &TmuxPaster)
}

/// `deliver_hail` with rung 4's seam supplied. Every other rung is the same.
pub fn deliver_hail_with(
    registry: &Registry,
    store: &Store,
    routes: &BTreeMap<String, Route>,
    message: &Message,
    paster: &dyn PanePaster,
) -> Result<Landing> {
    let route = routes.get(message.to.as_str());
    let harness = route.and_then(|route| route.harness);
    store.append_delivery_transition(
        &message.id,
        &message.to,
        harness,
        DeliveryState::Appended.as_str(),
        "mailbox",
        None,
        boop_harness::live::now_ms(),
    )?;
    let landing = land(registry, store, routes, message, paster)?;
    landing.record(store, &message.id, &message.to, harness)?;
    Ok(landing)
}

fn land(
    registry: &Registry,
    store: &Store,
    routes: &BTreeMap<String, Route>,
    message: &Message,
    paster: &dyn PanePaster,
) -> Result<Landing> {
    let to = message.to.as_str();
    let Some(route) = routes.get(to) else {
        return Ok(Landing::new(
            Rung::Mailbox,
            format!("no registry route for {to}"),
        ));
    };
    // A lane's own supervisor reads the mailbox directly and injects at its
    // next boundary, so the row is held rather than pushed at a door.
    if route.kind == "lane" {
        return Ok(Landing::new(Rung::TurnBoundary, "lane supervisor"));
    }
    let Some(id) = route.harness else {
        return Ok(no_door_route(
            route,
            to,
            paster,
            format!("route {to} names no harness"),
        ));
    };
    let harness = registry.get(id);
    if harness.capabilities().mail == MailPolicy::Keystrokes {
        return Ok(no_door_route(
            route,
            to,
            paster,
            "harness takes no door mail",
        ));
    }
    let Some(live) = live_session(harness, store, route, id)? else {
        return Ok(door_route_below_the_door(
            route,
            to,
            format!("no live {id} session for {to}"),
        ));
    };
    let (kind, addr) = door_columns(&live.door);
    store.record_live_door(&live.session_id, kind, addr.as_deref())?;
    Ok(match harness.door().deliver(&live, &message.body)? {
        Delivered::Injected => Landing::new(Rung::Door, "door"),
        Delivered::QueuedForTurnBoundary => Landing::new(Rung::TurnBoundary, "door queue"),
        Delivered::Unreachable(why) => door_route_below_the_door(route, to, why),
    })
}

/// A route whose harness owns a door, when that door answered nothing. The
/// hook inbox is the one drain the recipient itself runs; failing that the row
/// is held for the recipient's next turn boundary. A harness with a door is
/// never pasted into: a codex or claude TUI pane takes its mail through the
/// door or not at all, and typing at it puts keys in front of a human.
fn door_route_below_the_door(route: &Route, to: &str, why: impl Into<String>) -> Landing {
    let why = why.into();
    match hook_inbox(route, to) {
        true => Landing::new(Rung::HookInbox, why),
        false => Landing::new(Rung::TurnBoundary, why),
    }
}

/// A route with no door to try at all: no harness, or a harness whose only
/// transport was ever the pane. Rungs 3 through 5 in order.
fn no_door_route(
    route: &Route,
    to: &str,
    paster: &dyn PanePaster,
    why: impl Into<String>,
) -> Landing {
    let why = why.into();
    if hook_inbox(route, to) {
        return Landing::new(Rung::HookInbox, why);
    }
    match paste_into_pane(route, to, paster) {
        Some(pane) => Landing::new(Rung::PanePaste, format!("{why}; pane {pane}")),
        None => Landing::new(Rung::Mailbox, why),
    }
}

/// Whether the recipient's project carries an installed inbox hook.
fn hook_inbox(route: &Route, to: &str) -> bool {
    route
        .cwd
        .as_deref()
        .is_some_and(|cwd| crate::inbox::installed_for(std::path::Path::new(cwd), to))
}

/// Rung 4. The route's own pane takes the text as a paste when nothing else
/// answered. Returns the pane it reached, or `None` when no live pane exists.
/// The paste is one `send-keys` with no Enter: a human reads it and a TUI
/// prompt holds it, so nothing is submitted on the recipient's behalf.
fn paste_into_pane(route: &Route, to: &str, paster: &dyn PanePaster) -> Option<String> {
    let target = route.tmux.as_deref().filter(|target| !target.is_empty())?;
    if !boop_store::tmux::mux().target_alive(None, target) {
        return None;
    }
    let pane = pane_of_target(target).unwrap_or_else(|| target.to_owned());
    paster.paste(
        &pane,
        &format!("[boop] mail for {to}: run `boop inbox drain --me`"),
    )
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
