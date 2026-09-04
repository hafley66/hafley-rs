//! The delivery ladder: where one hail lands, in order, and the transition
//! each rung records. Every send path in boop walks this one function.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;

use boop_harness::door::Delivered;
use boop_harness::harness::{Harness, MailPolicy};
use boop_harness::live::{
    pane_of_target, DoorAddress, LiveSession, LiveSessionScope, LiveSessions, LiveStatus,
};
use boop_store::bus;
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
/// | `DoorQueue` | a live door session holds the text itself and reads it at its next turn boundary | accepted-by-harness |
/// | `Acpx` | the caller drives the recipient's own acpx queue | accepted-by-harness |
/// | `TurnBoundary` | the recipient's supervisor holds it, or a door harness whose door answered nothing holds it for its next turn | held-for-turn-boundary |
/// | `HookInbox` | the recipient's project carries an installed inbox hook | queued-in-hook-inbox |
/// | `PanePaste` | the route owns no door at all and names a live pane | pasted-into-pane |
/// | `Mailbox` | nothing answered; the row waits and the supervisor retries it | held-in-mailbox |
/// | `CoolOff` | the route's door budget is blown; the row waits out the cool-off and the drain retries it | cooled-off |
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Rung {
    Door,
    DoorQueue,
    Acpx,
    TurnBoundary,
    HookInbox,
    PanePaste,
    Mailbox,
    CoolOff,
}

impl Rung {
    /// The transition this rung records. One rung, one state.
    pub fn state(self) -> DeliveryState {
        match self {
            Rung::Door | Rung::DoorQueue | Rung::Acpx => DeliveryState::AcceptedByHarness,
            Rung::TurnBoundary => DeliveryState::HeldForTurnBoundary,
            Rung::HookInbox => DeliveryState::QueuedInHookInbox,
            Rung::PanePaste => DeliveryState::PastedIntoPane,
            Rung::Mailbox => DeliveryState::HeldInMailbox,
            Rung::CoolOff => DeliveryState::CooledOff,
        }
    }

    /// The word the sender prints, and the same word `boop debug` shows.
    pub fn as_str(self) -> &'static str {
        match self {
            Rung::Door => "door",
            Rung::DoorQueue => "door queue",
            Rung::Acpx => "acpx queue",
            Rung::TurnBoundary => "turn boundary",
            Rung::HookInbox => "hook inbox",
            Rung::PanePaste => "pane paste",
            Rung::Mailbox => "mailbox",
            Rung::CoolOff => "cool-off",
        }
    }

    /// Whether this rung put the message body itself in front of the
    /// recipient. The paste rung leaves a notice, never the body, so it does
    /// not ack the row: the recipient still drains it. A door queue holds the
    /// body inside the harness, so the row is acked the same as an injection:
    /// a drain that re-pushed it would put a second copy in front of the
    /// recipient (failure mode 14).
    pub fn carried_the_body(self) -> bool {
        matches!(self, Rung::Door | Rung::DoorQueue | Rung::Acpx)
    }
}

/// Where one message landed and why that rung. `detail` names the transport or
/// the check that sent the ladder one rung lower.
#[derive(Clone, Debug)]
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
    pub fn line(&self, message_id: &str, from: &str, to: &str, harness: &str) -> String {
        match self.rung {
            Rung::Door => format!("delivered {message_id} from {from} -> {to} through the {harness} door"),
            Rung::DoorQueue => format!(
                "delivered {message_id} from {from} -> {to} into the {harness} door queue; it reads it at its next turn boundary"
            ),
            Rung::Acpx => format!("delivered {message_id} from {from} -> {to} through the acpx queue"),
            Rung::TurnBoundary => format!(
                "held {message_id} from {from} -> {to} for the next turn boundary ({})",
                self.detail
            ),
            Rung::HookInbox => format!(
                "queued {message_id} from {from} -> {to} in the installed inbox hook ({})",
                self.detail
            ),
            Rung::PanePaste => format!(
                "pasted {message_id} from {from} -> {to} into its pane ({})",
                self.detail
            ),
            Rung::Mailbox => format!(
                "held {message_id} from {from} -> {to} in the mailbox ({}); the supervisor retries it",
                self.detail
            ),
            Rung::CoolOff => format!(
                "held {message_id} from {from} -> {to}: door budget blown ({}); the drain retries it after the cool-off",
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

/// How many door pushes one route may take inside one window (failure mode
/// 14, rail 2). The budget is the recipient's live connects: the lane routes
/// that name it as parent, floored so a coordinator with no lanes still takes
/// a human's hail. Past it the route is in a blowout and cools off; a body the
/// door already took this window is a replay and trips at once.
///
/// | field | default | env |
/// |---|---|---|
/// | `window` | 60 s | `BOOP_DOOR_WINDOW_SECS` |
/// | `cooldown` | 300 s | `BOOP_DOOR_COOLDOWN_SECS` |
/// | `floor` | 2 | `BOOP_DOOR_FLOOR` |
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoorBudget {
    pub window: Duration,
    pub cooldown: Duration,
    pub floor: usize,
}

impl Default for DoorBudget {
    fn default() -> Self {
        DoorBudget {
            window: Duration::from_secs(60),
            cooldown: Duration::from_secs(300),
            floor: 2,
        }
    }
}

impl DoorBudget {
    /// The defaults, each overridden by its env var when that parses.
    pub fn from_env() -> Self {
        let base = DoorBudget::default();
        let secs = |name: &str, fallback: Duration| {
            std::env::var(name)
                .ok()
                .and_then(|text| text.trim().parse::<u64>().ok())
                .map_or(fallback, Duration::from_secs)
        };
        DoorBudget {
            window: secs("BOOP_DOOR_WINDOW_SECS", base.window),
            cooldown: secs("BOOP_DOOR_COOLDOWN_SECS", base.cooldown),
            floor: std::env::var("BOOP_DOOR_FLOOR")
                .ok()
                .and_then(|text| text.trim().parse::<usize>().ok())
                .unwrap_or(base.floor),
        }
    }

    /// Pushes `route` may take per window: its registered lane children,
    /// never below the floor.
    pub fn allowance(&self, route: &str, routes: &BTreeMap<String, Route>) -> usize {
        crate::lane::children_of(route, routes)
            .len()
            .max(self.floor)
    }
}

/// What the budget says about one push at one route right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DoorVerdict {
    /// Under budget; push.
    Open,
    /// A trip is in force until `until_ms`; hold, record nothing new.
    CoolingOff { until_ms: u64 },
    /// This push would cross the budget; record the trip and hold.
    Blowout {
        pushes: usize,
        budget: usize,
        why: String,
    },
}

/// The budget check for one body at one route. Reads only the ledger, so
/// every boop process on the machine sees the same answer.
pub fn door_verdict(
    store: &Store,
    route: &str,
    routes: &BTreeMap<String, Route>,
    body: &str,
    budget: &DoorBudget,
    now_ms: u64,
) -> Result<DoorVerdict> {
    if let Some(trip) = store.latest_door_blowout(route)? {
        if trip.until_ms() > now_ms {
            return Ok(DoorVerdict::CoolingOff {
                until_ms: trip.until_ms(),
            });
        }
    }
    let window_ms = budget.window.as_millis() as u64;
    let since = now_ms.saturating_sub(window_ms);
    let pushes = store.door_pushes_since(route, since)?;
    let allowed = budget.allowance(route, routes);
    if pushes >= allowed {
        return Ok(DoorVerdict::Blowout {
            pushes,
            budget: allowed,
            why: format!(
                "{pushes} door pushes in {}s against {allowed} live connects",
                budget.window.as_secs()
            ),
        });
    }
    if store.door_pushed_body_since(route, body, since)? {
        return Ok(DoorVerdict::Blowout {
            pushes,
            budget: allowed,
            why: format!(
                "the same body already went through the door inside {}s",
                budget.window.as_secs()
            ),
        });
    }
    Ok(DoorVerdict::Open)
}

/// Whether `route` is inside a cool-off right now. A drain asks this once
/// per route and skips the whole route silently, so a cool-off writes no
/// transition per tick.
pub fn cooling_off(store: &Store, route: &str, now_ms: u64) -> bool {
    store
        .latest_door_blowout(route)
        .ok()
        .flatten()
        .is_some_and(|trip| trip.until_ms() > now_ms)
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
    deliver_hail_budgeted(registry, store, routes, message, paster, &DoorBudget::from_env())
}

/// `deliver_hail_with` with the door budget supplied, for a test that must
/// not read the environment.
pub fn deliver_hail_budgeted(
    registry: &Registry,
    store: &Store,
    routes: &BTreeMap<String, Route>,
    message: &Message,
    paster: &dyn PanePaster,
    budget: &DoorBudget,
) -> Result<Landing> {
    let route = routes.get(message.to.as_str());
    let harness = route.and_then(|route| route.harness);
    if !store.has_delivery_transition(&message.id)? {
        store.append_delivery_transition(
            &message.id,
            &message.to,
            harness,
            DeliveryState::Appended.as_str(),
            "mailbox",
            None,
            boop_harness::live::now_ms(),
        )?;
    }
    let landing = land(registry, store, routes, message, paster, budget)?;
    landing.record(store, &message.id, &message.to, harness)?;
    Ok(landing)
}

fn land(
    registry: &Registry,
    store: &Store,
    routes: &BTreeMap<String, Route>,
    message: &Message,
    paster: &dyn PanePaster,
    budget: &DoorBudget,
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
    let now_ms = boop_harness::live::now_ms();
    match door_verdict(store, to, routes, &message.body, budget, now_ms)? {
        DoorVerdict::Open => {}
        DoorVerdict::CoolingOff { until_ms } => {
            return Ok(Landing::new(
                Rung::CoolOff,
                format!("cooling off for {}s more", until_ms.saturating_sub(now_ms) / 1000),
            ));
        }
        DoorVerdict::Blowout {
            pushes,
            budget: allowed,
            why,
        } => {
            store.record_door_blowout(&boop_store::ident::DoorBlowoutRow {
                route: to.to_owned(),
                at_ms: now_ms,
                pushes,
                budget: allowed,
                window_ms: budget.window.as_millis() as u64,
                cooldown_ms: budget.cooldown.as_millis() as u64,
                why: why.clone(),
            })?;
            tracing::warn!(route = to, pushes, budget = allowed, %why, "door budget blown; cooling off");
            return Ok(Landing::new(Rung::CoolOff, why));
        }
    }
    Ok(match harness.door().deliver(&live, &message.body)? {
        Delivered::Injected => Landing::new(Rung::Door, "door"),
        Delivered::QueuedForTurnBoundary => Landing::new(Rung::DoorQueue, "door queue"),
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
        scope: boop_harness::live::LiveSessionScope::Unknown,
        parent_session: None,
    }
}

/// Claim markers older than a day are dead wrappers' leftovers.
const CLAIM_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// One registry-derived bind takes an exclusive marker: two wrappers waking
/// on one poll tick take two sessions, one each.
pub fn claim_open_session(dir: &Path, session_id: &str) -> bool {
    prune_claims(dir);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(format!("tui-claim-{session_id}")))
        .is_ok()
}

/// Delete claim markers past the claim TTL.
pub fn prune_claims(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("tui-claim-")
        {
            continue;
        }
        let age = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .map(|modified| modified.elapsed().unwrap_or_default());
        if age.is_ok_and(|age| age > CLAIM_TTL) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Sessions other registry routes already carry in one cwd, minus `my_pane`.
pub fn claimed_sessions(
    dir: &Path,
    my_pane: Option<&str>,
    canonical_cwd: &Path,
) -> BTreeSet<String> {
    let mut claimed = BTreeSet::new();
    let Ok(routes) = bus::read_routes(dir) else {
        return claimed;
    };
    for route in routes.into_values() {
        let Some(session) = route.session_id else {
            continue;
        };
        let Some(cwd) = route.cwd.as_deref() else {
            continue;
        };
        let route_cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| PathBuf::from(cwd));
        if route_cwd != canonical_cwd {
            continue;
        }
        let pane = route
            .tmux
            .as_deref()
            .map(|target| pane_of_target(target).unwrap_or_else(|| target.to_owned()));
        if pane.as_deref() == my_pane {
            continue;
        }
        claimed.insert(session);
    }
    claimed
}

/// Bind an unbound route to the one unclaimed live root session in its cwd.
/// Zero or several candidates bind nothing: wrong-session is worse than none.
pub fn bind_route_session(
    dir: &Path,
    route_name: &str,
    route: &mut Route,
    live: &dyn LiveSessions,
) -> bool {
    if route.session_id.is_some() || route.harness.is_none() {
        return false;
    }
    let Some(cwd) = route.cwd.as_deref() else {
        return false;
    };
    let canonical = std::fs::canonicalize(cwd).unwrap_or_else(|_| PathBuf::from(cwd));
    let claimed = claimed_sessions(dir, None, &canonical);
    let candidates: Vec<LiveSession> = live
        .live_sessions()
        .unwrap_or_default()
        .into_iter()
        .filter(|session| {
            session.scope != LiveSessionScope::Child
                && !claimed.contains(&session.session_id)
                && session
                    .cwd
                    .as_ref()
                    .map(|dir| std::fs::canonicalize(dir).unwrap_or_else(|_| dir.clone()))
                    .as_deref()
                    == Some(&canonical)
        })
        .collect();
    let [session] = candidates.as_slice() else {
        return false;
    };
    if !claim_open_session(dir, &session.session_id) {
        return false;
    }
    route.session_id = Some(session.session_id.clone());
    route.source_path = Some(format!("native-session={}", session.session_id));
    let _ = bus::write_route(dir, route_name, route);
    true
}

/// Re-push one route's parked rows, binding a session first when the
/// registry names exactly one. Door-taken rows are stamped, never replayed.
pub fn drain_route_held_mail(
    dir: &Path,
    registry: &Registry,
    store: &Store,
    route_name: &str,
) -> usize {
    drain_route_held_mail_budgeted(dir, registry, store, route_name, &DoorBudget::from_env())
}

/// `drain_route_held_mail` with the door budget supplied. A route inside a
/// cool-off is skipped whole and writes nothing; the first trip inside the
/// loop ends the pass, so one tick records at most one `cooled-off` row.
pub fn drain_route_held_mail_budgeted(
    dir: &Path,
    registry: &Registry,
    store: &Store,
    route_name: &str,
    budget: &DoorBudget,
) -> usize {
    let Some(mut route) = bus::read_routes(dir).ok().and_then(|mut routes| routes.remove(route_name))
    else {
        return 0;
    };
    if route.kind == "lane" {
        return 0; // the lane supervisor reads its own rows
    }
    if cooling_off(store, route_name, boop_harness::live::now_ms()) {
        return 0;
    }
    if route.session_id.is_none() {
        if let Some(harness) = route.harness {
            bind_route_session(dir, route_name, &mut route, registry.get(harness).live());
        }
    }
    let Ok(held) = bus::held_messages(store, route_name) else {
        return 0;
    };
    let routes = match bus::read_routes(dir) {
        Ok(routes) => routes,
        Err(_) => return 0,
    };
    let mut pushed = 0usize;
    for message in held {
        let Ok(landing) =
            deliver_hail_budgeted(registry, store, &routes, &message, &TmuxPaster, budget)
        else {
            continue;
        };
        if landing.rung == Rung::CoolOff {
            break;
        }
        if landing.rung.carried_the_body() {
            let _ = bus::ack_messages(store, std::slice::from_ref(&message.id), &bus::now_iso());
            pushed += 1;
        }
    }
    pushed
}

/// Bind and re-push for every non-lane route: one pass per sync-carrying
/// command and one pass per wrapper tick keep "read your mail" unnecessary.
pub fn drain_all_held_mail(dir: &Path, registry: &Registry, store: &Store) -> usize {
    let Ok(routes) = bus::read_routes(dir) else {
        return 0;
    };
    let mut pushed = 0usize;
    for name in routes.into_keys() {
        pushed += drain_route_held_mail(dir, registry, store, &name);
    }
    pushed
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
    use super::*;
    use boop_harness::door::{Door, IdleNotice};
    use boop_harness::harness::{Capabilities, ReadChunk, SessionRef};
    use boop_harness::live::DoorAddress;
    use std::time::Duration;

    /// A claude door that always takes the row, so the test measures which
    /// rung the ladder stops on rather than a real socket. It answers what the
    /// real claude door answers (`door/claude.rs` `deliver`): the harness
    /// queues the body for its next turn boundary. A double that answered
    /// `Injected` here blessed a drain that re-pushed every queued row
    /// (failure mode 14). Every body it takes is appended to `DOOR_LOG`.
    struct FakeClaudeDoor;

    static DOOR_LOG: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

    fn door_log() -> Vec<String> {
        DOOR_LOG.lock().unwrap().clone()
    }

    impl Door for FakeClaudeDoor {
        fn deliver(&self, _session: &LiveSession, body: &str) -> Result<Delivered> {
            DOOR_LOG.lock().unwrap().push(body.to_owned());
            Ok(Delivered::QueuedForTurnBoundary)
        }

        fn notify_idle(&self, _session: &LiveSession, _timeout: Duration) -> Result<IdleNotice> {
            Ok(IdleNotice::now(None))
        }
    }

    struct FakeClaudeLive;

    impl boop_harness::live::LiveSessions for FakeClaudeLive {
        fn live_sessions(&self) -> Result<Vec<LiveSession>> {
            Ok(vec![LiveSession {
                harness: HarnessId::Claude,
                session_id: "ses-fake-claude".to_owned(),
                pid: Some(4242),
                cwd: None,
                tmux_pane: Some("%77".to_owned()),
                status: LiveStatus::Idle,
                door: DoorAddress::UnixSocket {
                    path: "/tmp/boop-fake-claude.sock".into(),
                    token: None,
                },
                observed_ms: boop_harness::live::now_ms(),
                started_ms: None,
                scope: boop_harness::live::LiveSessionScope::Unknown,
                parent_session: None,
            }])
        }
    }

    /// Claude's own capabilities behind a door the test owns.
    struct FakeClaude;

    static FAKE_DOOR: FakeClaudeDoor = FakeClaudeDoor;
    static FAKE_LIVE: FakeClaudeLive = FakeClaudeLive;

    impl Harness for FakeClaude {
        fn id(&self) -> HarnessId {
            HarnessId::Claude
        }

        fn capabilities(&self) -> &'static Capabilities {
            boop_harness::harness::claude::Claude.capabilities()
        }

        fn live(&self) -> &dyn boop_harness::live::LiveSessions {
            &FAKE_LIVE
        }

        fn door(&self) -> &dyn Door {
            &FAKE_DOOR
        }

        fn sessions(&self) -> Result<Vec<SessionRef>> {
            Ok(Vec::new())
        }

        fn read_from(&self, _session: &SessionRef, offset: u64) -> Result<ReadChunk> {
            Ok(ReadChunk {
                events: Vec::new(),
                next_offset: offset,
                reset: false,
                skipped: 0,
            })
        }
    }

    /// Nothing paste-able; a paste here would mean the ladder fell past the
    /// door for a harness that owns one.
    struct NoPane;

    impl PanePaster for NoPane {
        fn paste(&self, _pane: &str, _notice: &str) -> Option<String> {
            panic!("a claude coordinator is never pasted into");
        }
    }

    fn message(to: &str) -> Message {
        Message {
            id: format!("m-{to}"),
            from: "wave-b-parent".to_owned(),
            to: to.to_owned(),
            from_timestamp: "2026-08-25T00:00:00Z".to_owned(),
            to_timestamp: None,
            kind: "request".to_owned(),
            reply_to: None,
            body: "a row for the coordinator".to_owned(),
            r#ref: None,
            rc: None,
            detail: None,
        }
    }

    /// A pane-less root session in the test cwd, the shape a bare codex
    /// outside tmux presents once its first turn exists.
    fn root_session(id: &str, cwd: PathBuf) -> LiveSession {
        LiveSession {
            harness: HarnessId::Claude,
            session_id: id.to_owned(),
            pid: Some(7),
            cwd: Some(cwd),
            tmux_pane: None,
            status: LiveStatus::Idle,
            door: DoorAddress::UnixSocket {
                path: "/tmp/boop-bound.sock".into(),
                token: None,
            },
            observed_ms: 0,
            started_ms: None,
            scope: LiveSessionScope::Root,
            parent_session: None,
        }
    }

    struct OneLive(PathBuf);

    impl LiveSessions for OneLive {
        fn live_sessions(&self) -> Result<Vec<LiveSession>> {
            Ok(vec![root_session("ses-one", self.0.clone())])
        }
    }

    struct TwoLive(PathBuf);

    impl LiveSessions for TwoLive {
        fn live_sessions(&self) -> Result<Vec<LiveSession>> {
            Ok(vec![
                root_session("ses-one", self.0.clone()),
                root_session("ses-two", self.0.clone()),
            ])
        }
    }

    fn unbound_route(cwd: &Path) -> Route {
        Route {
            kind: "coordinator".to_owned(),
            harness: Some(HarnessId::Claude),
            tmux: None,
            cwd: Some(cwd.display().to_string()),
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

    /// RECEIPT. A pane-less route binds the one candidate; a second route
    /// binds nothing because the first claim took the only session.
    #[test]
    fn a_paneless_route_binds_one_candidate_and_never_two() {
        let dir = std::env::temp_dir().join(format!("boop-bind-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut route = unbound_route(&dir);
        assert!(bind_route_session(&dir, "agent-a", &mut route, &OneLive(dir.clone())));
        assert_eq!(route.session_id.as_deref(), Some("ses-one"));

        let mut second = unbound_route(&dir);
        assert!(!bind_route_session(&dir, "agent-b", &mut second, &OneLive(dir.clone())));

        let mut third = unbound_route(&dir);
        assert!(!bind_route_session(&dir, "agent-c", &mut third, &TwoLive(dir.clone())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. A held row leaves the mailbox the first drain after its
    /// route's door can take it, stamped so no read replays it.
    #[test]
    fn held_mail_pushes_itself_once_the_route_can_take_it() {
        let dir = std::env::temp_dir().join(format!("boop-drain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let registry = Registry::with(vec![Box::new(FakeClaude)]);
        let mut route = unbound_route(&dir);
        route.session_id = Some("ses-fake-claude".to_owned());
        route.tmux = Some("%77".to_owned());
        bus::write_route(&dir, "claude-bare", &route).unwrap();

        let message = Message {
            id: "m-drain".to_owned(),
            from: "wave-b-parent".to_owned(),
            to: "claude-bare".to_owned(),
            from_timestamp: "2026-09-03T00:00:00Z".to_owned(),
            to_timestamp: None,
            kind: "request".to_owned(),
            reply_to: None,
            body: "push me".to_owned(),
            r#ref: None,
            rc: None,
            detail: None,
        };
        bus::append(&dir, "bus", &message).unwrap();
        let store = bus::open_store(&dir).unwrap();
        assert_eq!(bus::held_messages(&store, "claude-bare").unwrap().len(), 1);

        let pushed = drain_route_held_mail(&dir, &registry, &store, "claude-bare");
        assert_eq!(pushed, 1, "the held row leaves through the claude door");
        let taken = bus::messages_in(&store)
            .unwrap()
            .into_iter()
            .find(|row| row.id == "m-drain")
            .unwrap();
        assert!(taken.to_timestamp.is_some(), "the row is stamped taken");
        assert_eq!(
            bus::held_messages(&store, "claude-bare").unwrap().len(),
            0,
            "a second drain never replays a taken row"
        );
        for _ in 0..3 {
            assert_eq!(drain_route_held_mail(&dir, &registry, &store, "claude-bare"), 0);
            assert_eq!(drain_all_held_mail(&dir, &registry, &store), 0);
        }
        let copies = door_log().iter().filter(|body| body.as_str() == "push me").count();
        assert_eq!(copies, 1, "the door took the body exactly once over seven drains");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn budget(window_ms: u64, cooldown_ms: u64, floor: usize) -> DoorBudget {
        DoorBudget {
            window: Duration::from_millis(window_ms),
            cooldown: Duration::from_millis(cooldown_ms),
            floor,
        }
    }

    /// A coordinator route bound to the fake claude session, with `lanes`
    /// child lane routes naming it as parent, and `bodies` held rows.
    fn burst_fixture(tag: &str, lanes: usize, bodies: &[&str]) -> (PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!("boop-burst-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let name = format!("claude-{tag}");
        let mut route = unbound_route(&dir);
        route.session_id = Some("ses-fake-claude".to_owned());
        route.tmux = Some("%77".to_owned());
        bus::write_route(&dir, &name, &route).unwrap();
        for index in 0..lanes {
            let mut lane = unbound_route(&dir);
            lane.kind = "lane".to_owned();
            lane.parent = Some(name.clone());
            bus::write_route(&dir, &format!("{tag}-lane-{index}"), &lane).unwrap();
        }
        for (index, body) in bodies.iter().enumerate() {
            bus::append(
                &dir,
                "bus",
                &Message {
                    id: format!("m-{tag}-{index}"),
                    from: format!("{tag}-lane-0"),
                    to: name.clone(),
                    from_timestamp: "2026-09-03T00:00:00Z".to_owned(),
                    to_timestamp: None,
                    kind: "result".to_owned(),
                    reply_to: None,
                    body: (*body).to_owned(),
                    r#ref: None,
                    rc: None,
                    detail: None,
                },
            )
            .unwrap();
        }
        let store = bus::open_store(&dir).unwrap();
        (dir, store)
    }

    fn transitions(store: &Store, message_id: &str) -> Vec<(String, String)> {
        store
            .delivery_rows(message_id)
            .unwrap()
            .into_iter()
            .map(|row| (row.outcome, row.detail))
            .collect()
    }

    /// RECEIPT (failure mode 14, rail 2). Six rows for a coordinator with one
    /// live lane: the budget is the floor (2), so two go through the door,
    /// the third trips the breaker, and every later tick inside the cool-off
    /// pushes nothing and writes nothing.
    #[test]
    fn a_burst_past_the_recipients_live_connects_trips_and_cools_off() {
        let (dir, store) = burst_fixture(
            "burst",
            1,
            &["b-one", "b-two", "b-three", "b-four", "b-five", "b-six"],
        );
        let registry = Registry::with(vec![Box::new(FakeClaude)]);
        let budget = budget(60_000, 60_000, 2);

        let pushed =
            drain_route_held_mail_budgeted(&dir, &registry, &store, "claude-burst", &budget);
        assert_eq!(pushed, 2, "the budget is the floor for one live lane");
        let taken: Vec<_> = door_log()
            .into_iter()
            .filter(|body| body.starts_with("b-"))
            .collect();
        assert_eq!(taken, ["b-one", "b-two"]);

        let trips = store.door_blowouts("claude-burst").unwrap();
        assert_eq!(trips.len(), 1, "{trips:?}");
        assert_eq!((trips[0].pushes, trips[0].budget), (2, 2));
        assert!(trips[0].why.contains("2 door pushes in 60s against 2 live connects"), "{}", trips[0].why);

        assert_eq!(
            transitions(&store, "m-burst-2"),
            [
                ("appended".to_owned(), "mailbox".to_owned()),
                ("cooled-off".to_owned(), trips[0].why.clone()),
            ]
        );
        let untouched = [("appended".to_owned(), "mailbox".to_owned())];
        for later in ["m-burst-3", "m-burst-4", "m-burst-5"] {
            assert_eq!(transitions(&store, later), untouched, "{later} was touched past the trip");
        }
        assert_eq!(bus::held_messages(&store, "claude-burst").unwrap().len(), 4);

        let (_, before) = store
            .passthrough("SELECT COUNT(*) AS n FROM agent_delivery_transition")
            .unwrap();
        for _ in 0..5 {
            assert_eq!(
                drain_route_held_mail_budgeted(&dir, &registry, &store, "claude-burst", &budget),
                0
            );
        }
        let (_, after) = store
            .passthrough("SELECT COUNT(*) AS n FROM agent_delivery_transition")
            .unwrap();
        assert_eq!(before, after, "a cooling route writes no transition per tick");
        assert_eq!(store.door_blowouts("claude-burst").unwrap().len(), 1);
        assert_eq!(
            door_log().iter().filter(|body| body.starts_with("b-")).count(),
            2,
            "the door took nothing during the cool-off"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. Once the cool-off ends the drain drips at the budget again:
    /// five rows, floor 2, land as 2 + 2 + 1 across three windows.
    #[test]
    fn a_cool_off_ends_and_the_drip_resumes_at_budget() {
        let (dir, store) = burst_fixture("drip", 0, &["d-1", "d-2", "d-3", "d-4", "d-5"]);
        let registry = Registry::with(vec![Box::new(FakeClaude)]);
        let budget = budget(300, 300, 2);
        let mut per_pass = Vec::new();
        for _ in 0..3 {
            per_pass.push(drain_route_held_mail_budgeted(
                &dir,
                &registry,
                &store,
                "claude-drip",
                &budget,
            ));
            std::thread::sleep(Duration::from_millis(400));
        }
        assert_eq!(per_pass, [2, 2, 1]);
        let taken: Vec<_> = door_log()
            .into_iter()
            .filter(|body| body.starts_with("d-"))
            .collect();
        assert_eq!(taken, ["d-1", "d-2", "d-3", "d-4", "d-5"]);
        assert_eq!(store.door_blowouts("claude-drip").unwrap().len(), 2);
        assert!(bus::held_messages(&store, "claude-drip").unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. A body the door took this window is a replay whatever its id:
    /// it trips at once, under budget.
    #[test]
    fn a_body_the_door_already_took_this_window_trips_at_once() {
        let (dir, store) = burst_fixture("replay", 0, &["r-same", "r-same", "r-other"]);
        let registry = Registry::with(vec![Box::new(FakeClaude)]);
        let budget = budget(60_000, 60_000, 10);

        let pushed =
            drain_route_held_mail_budgeted(&dir, &registry, &store, "claude-replay", &budget);
        assert_eq!(pushed, 1);
        let trip = store.latest_door_blowout("claude-replay").unwrap().unwrap();
        assert!(trip.why.contains("same body"), "{}", trip.why);
        assert_eq!(
            door_log().iter().filter(|body| body.as_str() == "r-same").count(),
            1
        );
        assert_eq!(
            transitions(&store, "m-replay-2"),
            [("appended".to_owned(), "mailbox".to_owned())],
            "the row after the trip was touched"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (failure mode 14). A row the ledger shows a door already took,
    /// in the pre-fix shape (`held-for-turn-boundary` / `door queue`, no
    /// stamp), is never offered to the door again. The live store held 752
    /// such rows on 2026-09-03; one coordinator had each of its 22 pushed 29
    /// times.
    #[test]
    fn a_row_a_door_already_queued_is_never_pushed_again() {
        let dir = std::env::temp_dir().join(format!("boop-requeue-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let registry = Registry::with(vec![Box::new(FakeClaude)]);
        let mut route = unbound_route(&dir);
        route.session_id = Some("ses-fake-claude".to_owned());
        route.tmux = Some("%77".to_owned());
        bus::write_route(&dir, "claude-old", &route).unwrap();

        let message = Message {
            id: "m-legacy".to_owned(),
            from: "lane-x".to_owned(),
            to: "claude-old".to_owned(),
            from_timestamp: "2026-09-03T00:00:00Z".to_owned(),
            to_timestamp: None,
            kind: "result".to_owned(),
            reply_to: None,
            body: "already in front of you".to_owned(),
            r#ref: None,
            rc: None,
            detail: None,
        };
        bus::append(&dir, "bus", &message).unwrap();
        let store = bus::open_store(&dir).unwrap();
        for (outcome, detail) in [("appended", "mailbox"), ("held-for-turn-boundary", "door queue")] {
            store
                .append_delivery_transition(
                    "m-legacy",
                    "claude-old",
                    Some(HarnessId::Claude),
                    outcome,
                    detail,
                    None,
                    boop_harness::live::now_ms(),
                )
                .unwrap();
        }

        assert!(
            bus::held_messages(&store, "claude-old").unwrap().is_empty(),
            "a door-queued row is not held, whatever its latest outcome word"
        );
        assert_eq!(drain_route_held_mail(&dir, &registry, &store, "claude-old"), 0);
        assert!(
            !door_log().iter().any(|body| body == "already in front of you"),
            "the door was handed a row it already holds"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. A claude coordinator whose project carries no
    /// `.claude/settings.json` hook still takes its row at the door, so the
    /// hook inbox is a rung below rather than a step a caller installs.
    #[test]
    fn a_claude_coordinator_takes_its_row_at_the_door_with_no_hooks_installed() {
        let dir = std::env::temp_dir().join(format!("boop-door-only-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(
            !dir.join(".claude").join("settings.json").exists(),
            "the probe project carries no installed hook"
        );

        let store = Store::open(dir.join("store.db")).unwrap();
        let registry = Registry::with(vec![Box::new(FakeClaude)]);
        let mut routes = BTreeMap::new();
        routes.insert(
            "claude-77".to_owned(),
            Route {
                kind: "coordinator".to_owned(),
                harness: Some(HarnessId::Claude),
                tmux: Some("%77".to_owned()),
                cwd: Some(dir.display().to_string()),
                session_id: Some("ses-fake-claude".to_owned()),
                model: None,
                mode: None,
                source_path: None,
                parent: None,
                goal: None,
                registered_at: None,
                base_sha: None,
                worktree_dir: None,
                app_server_socket: None,
            },
        );

        let landing =
            deliver_hail_with(&registry, &store, &routes, &message("claude-77"), &NoPane).unwrap();
        assert_eq!(landing.rung, Rung::DoorQueue, "{landing:?}");
        assert!(landing.rung.carried_the_body());
        assert_eq!(
            landing.line("m-1", "coordinator", "claude-77", "claude"),
            "delivered m-1 from coordinator -> claude-77 into the claude door queue; it reads it at its next turn boundary"
        );

        let (_, history) = store
            .passthrough(
                "SELECT outcome FROM agent_delivery_transition \
                 WHERE message_id = 'm-claude-77' ORDER BY sequence",
            )
            .unwrap();
        let states: Vec<String> = history
            .iter()
            .map(|row| row["outcome"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert_eq!(
            states,
            vec![
                DeliveryState::Appended.as_str().to_owned(),
                DeliveryState::AcceptedByHarness.as_str().to_owned()
            ],
            "{history:#?}"
        );
        assert!(
            !states.iter().any(|state| state.contains("hook-inbox")),
            "no hook rung was walked: {states:?}"
        );
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }

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
