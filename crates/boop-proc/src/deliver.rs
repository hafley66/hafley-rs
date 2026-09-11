//! The delivery ladder: where one hail lands, in order, and the transition
//! each rung records. Every send path in boop walks this one function.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;

use boop_harness::door::Delivered;
use boop_harness::harness::{Harness, MailPolicy};
use boop_harness::live::{pane_of_target, DoorAddress, LiveSession, LiveSessions, LiveStatus};
use boop_harness::Registry;
use boop_store::bus;
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
/// | `DoorQueue` | a live door session accepted the text into its queue for its next turn boundary | held-for-turn-boundary |
/// | `Acpx` | the caller drives the recipient's own acpx queue | accepted-by-harness |
/// | `TurnBoundary` | the recipient's own lane supervisor holds it, which is the one rung with a real holder | held-for-turn-boundary |
/// | `HookInbox` | the recipient's project carries an installed inbox hook | queued-in-hook-inbox |
/// | `PanePaste` | the route owns no door at all and names a live pane | pasted-into-pane |
/// | `MailboxOnly` | a supervisor's progress row about a lane's run; a lane's end row takes the door like a hail | held-in-mailbox |
/// | `Mailbox` | nothing answered; the row stays unread and the drain retries it | held-in-mailbox |
/// | `CoolOff` | the route's door budget is blown; the row waits out the cool-off and the drain retries it | cooled-off |
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Rung {
    AlreadyAccepted,
    Door,
    DoorQueue,
    Acpx,
    TurnBoundary,
    HookInbox,
    PanePaste,
    MailboxOnly,
    Mailbox,
    CoolOff,
}

impl Rung {
    /// The transition this rung records. One rung, one state.
    pub fn state(self) -> DeliveryState {
        match self {
            Rung::AlreadyAccepted | Rung::Door | Rung::Acpx => DeliveryState::AcceptedByHarness,
            Rung::DoorQueue | Rung::TurnBoundary => DeliveryState::HeldForTurnBoundary,
            Rung::HookInbox => DeliveryState::QueuedInHookInbox,
            Rung::PanePaste => DeliveryState::PastedIntoPane,
            Rung::MailboxOnly | Rung::Mailbox => DeliveryState::HeldInMailbox,
            Rung::CoolOff => DeliveryState::CooledOff,
        }
    }

    /// The word the sender prints, and the same word `boop debug` shows.
    pub fn as_str(self) -> &'static str {
        match self {
            Rung::AlreadyAccepted => "already accepted",
            Rung::Door => "door",
            Rung::DoorQueue => "door queue",
            Rung::Acpx => "acpx queue",
            Rung::TurnBoundary => "turn boundary",
            Rung::HookInbox => "hook inbox",
            Rung::PanePaste => "pane paste",
            Rung::MailboxOnly => "mailbox only",
            Rung::Mailbox => "mailbox",
            Rung::CoolOff => "cool-off",
        }
    }

    /// Whether this rung put the message body itself in front of the
    /// recipient. The paste rung leaves a notice, never the body, so it does
    /// not ack the row: the recipient still drains it. A door queue holds the
    /// body inside the harness, so the row is acked the same as an injection:
    /// a drain that re-pushed it would put a second copy in front of the
    /// recipient (failure mode 14). `deliver_hail_budgeted` reads this and
    /// stamps the row, so every caller of the ladder stamps alike.
    pub fn carried_the_body(self) -> bool {
        matches!(
            self,
            Rung::AlreadyAccepted | Rung::Door | Rung::DoorQueue | Rung::Acpx
        )
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
            Rung::AlreadyAccepted => format!("already accepted {message_id} from {from} -> {to}; no new transport call"),
            Rung::Door => format!("delivered {message_id} from {from} -> {to} through the {harness} door"),
            Rung::DoorQueue => format!(
                "queued {message_id} from {from} -> {to} in the {harness} door; it reads it at its next turn boundary"
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
            Rung::MailboxOnly => format!(
                "held {message_id} from {from} -> {to} in the mailbox ({}); {to} reads it with `boop wait`",
                self.detail
            ),
            Rung::Mailbox => format!(
                "held {message_id} from {from} -> {to} in the mailbox ({}); {to} reads it with `boop wait --me` and the drain retries it",
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

    /// Whether `target` names a pane this paster can reach. The default asks
    /// the mux; a paster that must not touch a real terminal answers itself.
    fn alive(&self, target: &str) -> bool {
        boop_store::tmux::mux().target_alive(None, target)
    }
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
/// | `floor` | 32 | `BOOP_DOOR_FLOOR` |
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
            floor: 32,
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

/// The verdict with its bookkeeping, for every path that pushes at a door:
/// the ladder, the children fan-out, and the acpx queue. `None` means push.
/// `Some` is the cool-off landing to record instead; a fresh trip writes its
/// `agent_door_blowout` row here, a route already cooling off writes nothing.
pub fn door_gate(
    store: &Store,
    route: &str,
    routes: &BTreeMap<String, Route>,
    body: &str,
    budget: &DoorBudget,
    now_ms: u64,
) -> Result<Option<Landing>> {
    Ok(
        match door_verdict(store, route, routes, body, budget, now_ms)? {
            DoorVerdict::Open => None,
            DoorVerdict::CoolingOff { until_ms } => Some(Landing::new(
                Rung::CoolOff,
                format!(
                    "cooling off for {}s more",
                    until_ms.saturating_sub(now_ms) / 1000
                ),
            )),
            DoorVerdict::Blowout {
                pushes,
                budget: allowed,
                why,
            } => {
                store.record_door_blowout(&boop_store::ident::DoorBlowoutRow {
                    route: route.to_owned(),
                    at_ms: now_ms,
                    pushes,
                    budget: allowed,
                    window_ms: budget.window.as_millis() as u64,
                    cooldown_ms: budget.cooldown.as_millis() as u64,
                    why: why.clone(),
                })?;
                tracing::warn!(route, pushes, budget = allowed, %why, "door budget blown; cooling off");
                Some(Landing::new(Rung::CoolOff, why))
            }
        },
    )
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
    deliver_hail_budgeted(
        registry,
        store,
        routes,
        message,
        paster,
        &DoorBudget::from_env(),
    )
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
    // Admission spans the external call and its receipt, without keeping a
    // SQLite write transaction open across transport I/O. File locks are
    // released on process exit; in-memory stores have no shared file owner.
    let _admission = match store.connection().path().filter(|path| !path.is_empty()) {
        Some(path) => match bus::try_route_lock(Path::new(path), &message.to, "delivery")? {
            Some(lock) => Some(lock),
            None => {
                // Nothing holds this row, so it stays unread for `wait --me`.
                let held = Landing::new(Rung::Mailbox, "another delivery attempt is in flight");
                held.record(store, &message.id, &message.to, harness)?;
                return Ok(held);
            }
        },
        None => None,
    };
    if store.delivery_accepted(&message.id, &message.to)? {
        bus::ack_messages(store, std::slice::from_ref(&message.id), &bus::now_iso())?;
        return Ok(Landing::new(
            Rung::AlreadyAccepted,
            "previously accepted by harness or its queue",
        ));
    }
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
    // The ladder stamps the row itself. A rung that carried the body put the
    // text in front of the recipient, so the mailbox row is history: leaving
    // it open is how the supervisor's parent rows sat unstamped while the
    // ledger already said the door or its queue accepted it, invisible to both
    // `held_messages` and `boop wait --me` (head-rewound-door-retry).
    if landing.rung.carried_the_body() {
        bus::ack_messages(store, std::slice::from_ref(&message.id), &bus::now_iso())?;
        // One push per (lane, subscriber, head): the commit row joins the
        // ledger the moment the door takes it, so a same-head replay in the
        // window is held rather than offered to the door again.
        if message.kind.commit_row() {
            store.record_commit_push(
                &message.from,
                &message.to,
                &commit_head(&message.body),
                &message.id,
                boop_harness::live::now_ms(),
            )?;
        }
    }
    Ok(landing)
}

/// Whether a commit row pushes at `subscriber`'s door or stays in the mailbox.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum CommitPush {
    Door,
    Mailbox,
}

/// The mode a commit row to `subscriber` about `lane` follows: an exact
/// `agent_commit_subscription` row, then its `'*'` row, else the subscriber's
/// route kind. A coordinator, a native route, or an `acpx` route takes the
/// door by default; a lane parent and an unknown route keep the mailbox.
pub fn commit_push_mode(
    store: &Store,
    routes: &BTreeMap<String, Route>,
    subscriber: &str,
    lane: &str,
) -> CommitPush {
    match store.commit_subscription(subscriber, lane) {
        Ok(Some(mode)) if mode == "mailbox" => CommitPush::Mailbox,
        Ok(Some(_)) => CommitPush::Door,
        Ok(None) | Err(_) => default_commit_push(routes, subscriber),
    }
}

/// The mode a subscriber with no explicit subscription row follows.
fn default_commit_push(routes: &BTreeMap<String, Route>, subscriber: &str) -> CommitPush {
    match routes.get(subscriber) {
        Some(route)
            if route.kind == "coordinator"
                || route.kind == "native"
                || route.mode.as_deref() == Some("acpx") =>
        {
            CommitPush::Door
        }
        _ => CommitPush::Mailbox,
    }
}

/// The new head sha a commit row's body travels in: the tail of its `a..b`
/// range token. An unrecognised body yields the empty string.
fn commit_head(body: &str) -> String {
    body.split_whitespace()
        .find_map(|token| token.split_once("..").map(|(_, new)| new.to_owned()))
        .unwrap_or_default()
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
    // A commit row is a progress row, but a wip commit may take the door when
    // its subscriber asked for one. A done commit, a mailbox mode, and a head
    // already pushed all stop here; only a fresh door push falls through.
    if message.kind.commit_row() {
        let head = commit_head(&message.body);
        if message.detail.as_deref() == Some("done") {
            return Ok(Landing::new(
                Rung::MailboxOnly,
                format!("commit {head} row; done stays in the mailbox"),
            ));
        }
        if commit_push_mode(store, routes, to, message.from.as_str()) == CommitPush::Mailbox {
            return Ok(Landing::new(
                Rung::MailboxOnly,
                "commit row; subscriber reads the mailbox",
            ));
        }
        if store.commit_push_exists(message.from.as_str(), to, &head)? {
            return Ok(Landing::new(
                Rung::MailboxOnly,
                format!("commit {head} already pushed to {to}"),
            ));
        }
    }
    // Rung 0, narrowed 2026-09-07: an end row pushes so a parent hears of a
    // death unasked; six lanes yielding flood a transcript (2026-09-05). A
    // commit row in Door mode already fell through above and takes the door.
    if message.kind.lane_progress_row() && !message.kind.commit_row() {
        return Ok(Landing::new(
            Rung::MailboxOnly,
            format!("{} row; no door", message.kind.as_str()),
        ));
    }
    let Some(route) = routes.get(to) else {
        return Ok(Landing::new(
            Rung::Mailbox,
            format!("no registry route for {to}"),
        ));
    };
    // A lane's own supervisor reads the mailbox directly and injects at its
    // next boundary, so the row is held rather than pushed at a door.
    if route.kind == "lane" {
        if hook_inbox(registry, route, to) {
            return Ok(Landing::new(Rung::HookInbox, "installed inbox hook"));
        }
        return Ok(Landing::new(Rung::TurnBoundary, "lane supervisor"));
    }
    if route.mode.as_deref() == Some("acpx") {
        if let Some(cooled) = door_gate(
            store,
            to,
            routes,
            &message.body,
            budget,
            boop_harness::live::now_ms(),
        )? {
            return Ok(cooled);
        }
        let reply = boop_acp::channel::acpx::prompt(route, &message.body, true)?;
        return Ok(Landing::acpx(reply.trim_end().to_owned()));
    }
    let Some(id) = route.harness else {
        return Ok(no_door_route(
            registry,
            route,
            to,
            paster,
            format!("route {to} names no harness"),
        ));
    };
    let harness = registry.get(id);
    if harness.capabilities().mail == MailPolicy::Keystrokes {
        return Ok(no_door_route(
            registry,
            route,
            to,
            paster,
            "harness takes no door mail",
        ));
    }
    // The live-session lookup is itself a door status read: a stopped server
    // parks here until its client timeout, so time it like the deliver call.
    let live_started = Instant::now();
    let live = live_session(harness, store, route, id)?;
    let live_elapsed_ms = live_started.elapsed().as_millis() as u64;
    if live_elapsed_ms >= SLOW_DOOR_CALL_MS {
        tracing::warn!(
            door = id.as_str(),
            route = to,
            elapsed_ms = live_elapsed_ms,
            "slow door call"
        );
    }
    let Some(live) = live else {
        return Ok(door_route_below_the_door(
            registry,
            route,
            to,
            format!("no live {id} session for {to}"),
        ));
    };
    let (kind, addr) = door_columns(&live.door);
    store.record_live_door(&live.session_id, kind, addr.as_deref())?;
    let now_ms = boop_harness::live::now_ms();
    if let Some(cooled) = door_gate(store, to, routes, &message.body, budget, now_ms)? {
        return Ok(cooled);
    }
    // The recipient reads pushed mail beside its own turns, so the row names
    // its sender through the same mood template a lane's supervisor uses.
    let rendered = crate::supervise::render_mail(
        &crate::supervise::mood_template(to),
        message.kind.as_str(),
        &message.id,
        &message.from,
        &message.body,
    );
    let door = id.as_str();
    let door_started = Instant::now();
    let delivered = harness.door().deliver(&live, &rendered);
    let door_elapsed_ms = door_started.elapsed().as_millis() as u64;
    if door_elapsed_ms >= SLOW_DOOR_CALL_MS {
        tracing::warn!(
            door,
            route = to,
            elapsed_ms = door_elapsed_ms,
            "slow door call"
        );
    }
    Ok(match delivered {
        Ok(Delivered::Injected) => Landing::new(Rung::Door, "door"),
        Ok(Delivered::QueuedForTurnBoundary) => Landing::new(Rung::DoorQueue, "door queue"),
        Ok(Delivered::Unreachable(why)) => {
            // Name the door and the route on every attempt so an operator
            // reads which transport failed without opening the store.
            let why = format!("{door} door for {to}: {why}");
            if door_transport_failure(&why) {
                record_door_failure(store, to, routes, &why, now_ms)?;
                tracing::warn!(door, route = to, elapsed_ms = door_elapsed_ms, %why, "door call failed");
            }
            door_route_below_the_door(registry, route, to, why)
        }
        Err(error) => {
            // An Err is a request that could not be formed or answered at all:
            // a transport failure, whatever the transport spells it. Only a
            // transport failure cools the route off; a live door that refused
            // to answer (an ambiguous steer receipt, say) must not bench the
            // route for the whole cool-off.
            let why = format!("{door} door for {to}: {error}");
            if door_transport_failure(&why) {
                record_door_failure(store, to, routes, &why, now_ms)?;
                tracing::warn!(door, route = to, elapsed_ms = door_elapsed_ms, error = %error, "door call failed");
            }
            door_route_below_the_door(registry, route, to, why)
        }
    })
}

/// The env knob for how long a route whose door failed to connect is skipped.
pub const DOOR_FAIL_COOLDOWN_ENV: &str = "BOOP_DOOR_FAIL_COOLDOWN_SECS";

/// A dead door is skipped this long, so a drain never re-walks it every tick.
const DOOR_FAIL_COOLDOWN_DEFAULT: Duration = Duration::from_secs(120);

/// A door status read or deliver past this is a slow external effect.
const SLOW_DOOR_CALL_MS: u64 = 1000;

fn door_failure_cooldown() -> Duration {
    std::env::var(DOOR_FAIL_COOLDOWN_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(DOOR_FAIL_COOLDOWN_DEFAULT)
}

/// Whether a door answer is a transport failure (dead or unreachable) rather
/// than a live door declining this body for now. A busy door is not cooled off.
fn door_transport_failure(why: &str) -> bool {
    let why = why.to_ascii_lowercase();
    [
        "connect",
        "socket",
        "refused",
        "unreachable",
        "app-server",
        "app server",
        "timed out",
        "timeout",
        "broken pipe",
        "connection reset",
        "no such file",
    ]
    .iter()
    .any(|needle| why.contains(needle))
}

/// Record a cool-off for a route whose door could not take the body, so the
/// drain skips it instead of re-walking the same dead transport every tick.
fn record_door_failure(
    store: &Store,
    route: &str,
    routes: &BTreeMap<String, Route>,
    why: &str,
    now_ms: u64,
) -> Result<()> {
    let budget = DoorBudget::from_env();
    store.record_door_blowout(&boop_store::ident::DoorBlowoutRow {
        route: route.to_owned(),
        at_ms: now_ms,
        pushes: 0,
        budget: budget.allowance(route, routes),
        window_ms: budget.window.as_millis() as u64,
        cooldown_ms: door_failure_cooldown().as_millis() as u64,
        why: format!("door-unreachable: {why}"),
    })?;
    tracing::warn!(
        route,
        %why,
        cooldown_secs = door_failure_cooldown().as_secs(),
        "door unreachable; cooling off the route"
    );
    Ok(())
}

/// A door that answered nothing. No supervisor holds a coordinator's mail, so
/// the row stays unread in the mailbox instead of claiming a turn boundary.
fn door_route_below_the_door(
    registry: &Registry,
    route: &Route,
    to: &str,
    why: impl Into<String>,
) -> Landing {
    let why = why.into();
    match hook_inbox(registry, route, to) {
        true => Landing::new(Rung::HookInbox, why),
        false => Landing::new(Rung::Mailbox, why),
    }
}

/// A route with no door to try at all: no harness, or a harness whose only
/// transport was ever the pane. Rungs 3 through 5 in order.
fn no_door_route(
    registry: &Registry,
    route: &Route,
    to: &str,
    paster: &dyn PanePaster,
    why: impl Into<String>,
) -> Landing {
    let why = why.into();
    if hook_inbox(registry, route, to) {
        return Landing::new(Rung::HookInbox, why);
    }
    match paste_into_pane(route, to, paster) {
        Some(pane) => Landing::new(Rung::PanePaste, format!("{why}; pane {pane}")),
        None => Landing::new(Rung::Mailbox, why),
    }
}

/// Whether the recipient's project carries an installed inbox hook.
pub fn hook_inbox(registry: &Registry, route: &Route, to: &str) -> bool {
    let Some(cwd) = route.cwd.as_deref() else {
        return false;
    };
    let cwd = std::path::Path::new(cwd);
    match route.harness {
        Some(id) => registry.get(id).door().inbox_hook_installed(cwd, to),
        None => registry
            .all()
            .iter()
            .any(|adapter| adapter.door().inbox_hook_installed(cwd, to)),
    }
}

/// Rung 4. The route's own pane takes the text as a paste when nothing else
/// answered. Returns the pane it reached, or `None` when no live pane exists.
/// The paste is one `send-keys` with no Enter: a human reads it and a TUI
/// prompt holds it, so nothing is submitted on the recipient's behalf.
fn paste_into_pane(route: &Route, to: &str, paster: &dyn PanePaster) -> Option<String> {
    let target = route.tmux.as_deref().filter(|target| !target.is_empty())?;
    if !paster.alive(target) {
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
    if let Some(live) = harness.live().live_session_for_route(route)? {
        return Ok(Some(live));
    }
    if route.mode.as_deref() == Some("native-owned") {
        return Ok(None);
    }
    let Some(session_id) = route.session_id.as_deref() else {
        return Ok(None);
    };
    Ok(store
        .live_row(session_id)?
        .filter(|row| !matches!(row.status.as_deref(), Some("detached" | "closed")))
        .map(|row| projected(id, row)))
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

/// Bind an unbound route only when the harness can resolve its explicit route
/// evidence, such as an exact pane or adapter-owned control endpoint.
pub fn bind_route_session(
    dir: &Path,
    route_name: &str,
    route: &mut Route,
    live: &dyn LiveSessions,
) -> bool {
    if route.session_id.is_some()
        || route.harness.is_none()
        || route.app_server_socket.is_some()
        || route.mode.as_deref() == Some("native-owned")
    {
        return false;
    }
    let Ok(Some(session)) = live.live_session_for_route(route) else {
        return false;
    };
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
    let Some(mut route) = bus::read_routes(dir)
        .ok()
        .and_then(|mut routes| routes.remove(route_name))
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
        // A progress row is never pushed, so re-walking the ladder would only
        // stamp a second `held-in-mailbox`; an end row retries like a hail. A
        // commit row in Door mode that is not done is the one progress row
        // that retries: the door may have been cooling off when it landed.
        let commit_retry = message.kind.commit_row()
            && message.detail.as_deref() != Some("done")
            && commit_push_mode(store, &routes, &message.to, &message.from) == CommitPush::Door;
        if message.kind.lane_progress_row() && !commit_retry {
            continue;
        }
        let Ok(landing) =
            deliver_hail_budgeted(registry, store, &routes, &message, &TmuxPaster, budget)
        else {
            continue;
        };
        if landing.rung == Rung::CoolOff {
            break;
        }
        // Nothing answered: every later row walks the same dead door, so the
        // pass ends here and the next tick retries from the oldest row.
        if landing.rung == Rung::Mailbox {
            break;
        }
        if landing.rung.carried_the_body() {
            pushed += 1; // `deliver_hail_budgeted` stamped the row
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
    let budget = drain_budget();
    let started = Instant::now();
    let total = routes.len();
    let mut pushed = 0usize;
    for (index, name) in routes.into_keys().enumerate() {
        if started.elapsed() >= budget {
            tracing::warn!(
                elapsed_ms = started.elapsed().as_millis() as u64,
                budget_ms = budget.as_millis() as u64,
                skipped = total - index,
                next_route = %name,
                "held-mail drain bailed at its deadline; set {DRAIN_BUDGET_ENV} to widen it"
            );
            break;
        }
        let route_started = Instant::now();
        pushed += drain_route_held_mail(dir, registry, store, &name);
        let route_ms = route_started.elapsed().as_millis() as u64;
        if route_ms >= SLOW_ROUTE_DRAIN.as_millis() as u64 {
            tracing::warn!(route = %name, elapsed_ms = route_ms, "slow held-mail drain for one route");
        }
    }
    pushed
}

/// The env knob for the whole drain pass's wall-clock budget, in ms.
pub const DRAIN_BUDGET_ENV: &str = "BOOP_DRAIN_BUDGET_MS";

/// A drain pass rides every sync-carrying read verb; past this it bails.
const DRAIN_BUDGET_DEFAULT: Duration = Duration::from_millis(1500);

/// One route's drain past this is reported as a slow external effect.
const SLOW_ROUTE_DRAIN: Duration = Duration::from_millis(250);

fn drain_budget() -> Duration {
    std::env::var(DRAIN_BUDGET_ENV)
        .ok()
        .and_then(|value| value.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(DRAIN_BUDGET_DEFAULT)
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

    /// The door log with the recipient's mood prefix stripped, so a body
    /// assertion names what the sender wrote.
    fn door_bodies() -> Vec<String> {
        door_log()
            .into_iter()
            .map(|line| match line.split_once("] ") {
                Some((_, body)) => body.to_owned(),
                None => line,
            })
            .collect()
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

        fn mock_tui_launch(
            &self,
            _: &boop_harness::harness::mock_tui::MockTuiContext<'_>,
        ) -> anyhow::Result<boop_harness::harness::mock_tui::MockTuiLaunch> {
            anyhow::bail!("fixture harness has no mock launch")
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
            kind: "request".into(),
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
            tmux_pane: Some("%77".into()),
            status: LiveStatus::Idle,
            door: DoorAddress::UnixSocket {
                path: "/tmp/boop-bound.sock".into(),
                token: None,
            },
            observed_ms: 0,
            started_ms: None,
            scope: boop_harness::live::LiveSessionScope::Root,
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
            kind: "coordinator".into(),
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

    /// RECEIPT. Same-cwd candidates cannot bind a route. An exact pane can.
    #[test]
    fn route_binding_requires_authoritative_route_evidence() {
        let dir = std::env::temp_dir().join(format!("boop-bind-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut observed = unbound_route(&dir);
        observed.mode = Some("native-owned".into());
        assert!(!bind_route_session(
            &dir,
            "observed-route",
            &mut observed,
            &OneLive(dir.clone())
        ));
        assert_eq!(observed.session_id, None);

        let mut route = unbound_route(&dir);
        assert!(!bind_route_session(
            &dir,
            "agent-a",
            &mut route,
            &OneLive(dir.clone())
        ));
        assert_eq!(route.session_id, None);

        let mut exact = unbound_route(&dir);
        exact.tmux = Some("%77".into());
        assert!(bind_route_session(
            &dir,
            "agent-b",
            &mut exact,
            &OneLive(dir.clone())
        ));
        assert_eq!(exact.session_id.as_deref(), Some("ses-one"));

        let mut third = unbound_route(&dir);
        assert!(!bind_route_session(
            &dir,
            "agent-c",
            &mut third,
            &TwoLive(dir.clone())
        ));
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
            kind: "request".into(),
            reply_to: None,
            body: "push me".to_owned(),
            r#ref: None,
            rc: None,
            detail: None,
        };
        bus::append(&dir, "bus", &message).unwrap();
        let first_process = bus::open_store(&dir).unwrap();
        assert_eq!(
            bus::held_messages(&first_process, "claude-bare")
                .unwrap()
                .len(),
            1
        );
        drop(first_process);

        let resumed_process = bus::open_store(&dir).unwrap();
        let pushed = drain_route_held_mail(&dir, &registry, &resumed_process, "claude-bare");
        assert_eq!(pushed, 1, "the held row leaves through the claude door");
        let taken = bus::messages_in(&resumed_process)
            .unwrap()
            .into_iter()
            .find(|row| row.id == "m-drain")
            .unwrap();
        assert!(taken.to_timestamp.is_some(), "the row is stamped taken");
        assert_eq!(
            bus::held_messages(&resumed_process, "claude-bare")
                .unwrap()
                .len(),
            0,
            "a second drain never replays a taken row"
        );
        drop(resumed_process);
        let later_process = bus::open_store(&dir).unwrap();
        for _ in 0..3 {
            assert_eq!(
                drain_route_held_mail(&dir, &registry, &later_process, "claude-bare"),
                0
            );
            assert_eq!(drain_all_held_mail(&dir, &registry, &later_process), 0);
        }
        let copies = door_bodies()
            .iter()
            .filter(|body| body.as_str() == "push me")
            .count();
        assert_eq!(
            copies, 1,
            "the door took the body exactly once over seven drains"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. A failed ACPX process leaves the durable mailbox row open. A
    /// later Boop process retries it, records remote acceptance, and later
    /// drains make no second transport call. This does not prove exactly-once
    /// across a crash after remote acceptance and before the local receipt.
    #[test]
    fn acpx_failure_retries_after_store_reopen_then_stays_accepted() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("boop-acpx-restart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let executable = dir.join("fake-acpx");
        let calls = dir.join("calls");
        let failing = format!("#!/bin/sh\nprintf x >> '{}'\nexit 7\n", calls.display());
        std::fs::write(&executable, failing).unwrap();
        let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o700);
        std::fs::set_permissions(&executable, permissions).unwrap();
        let previous = std::env::var_os("BOOP_ACPX_BIN");
        std::env::set_var("BOOP_ACPX_BIN", &executable);

        let mut route = unbound_route(&dir);
        route.mode = Some("acpx".into());
        route.session_id = Some("persistent-session".into());
        route.source_path = Some("acpx-agent=claude".into());
        bus::write_route(&dir, "acpx-parent", &route).unwrap();
        let message = Message {
            id: "m-acpx-restart".into(),
            from: "worker".into(),
            to: "acpx-parent".into(),
            from_timestamp: "2026-09-08T00:00:00Z".into(),
            to_timestamp: None,
            kind: "result".into(),
            reply_to: None,
            body: "completion-to-parent".into(),
            r#ref: None,
            rc: Some(0),
            detail: None,
        };
        bus::append(&dir, "bus", &message).unwrap();
        let registry = Registry::with(vec![Box::new(FakeClaude)]);
        let first_process = bus::open_store(&dir).unwrap();
        let routes = bus::read_routes(&dir).unwrap();
        assert!(deliver_hail_budgeted(
            &registry,
            &first_process,
            &routes,
            &message,
            &NoPane,
            &budget(60_000, 60_000, 10),
        )
        .is_err());
        assert_eq!(
            bus::held_messages(&first_process, "acpx-parent")
                .unwrap()
                .len(),
            1
        );
        assert_eq!(first_process.delivery_rows(&message.id).unwrap().len(), 1);
        drop(first_process);

        let succeeding = format!(
            "#!/bin/sh\nprintf x >> '{}'\nprintf accepted\n",
            calls.display()
        );
        std::fs::write(&executable, succeeding).unwrap();
        let resumed_process = bus::open_store(&dir).unwrap();
        assert_eq!(
            drain_route_held_mail(&dir, &registry, &resumed_process, "acpx-parent"),
            1
        );
        assert!(resumed_process
            .delivery_accepted(&message.id, "acpx-parent")
            .unwrap());
        assert!(bus::held_messages(&resumed_process, "acpx-parent")
            .unwrap()
            .is_empty());
        drop(resumed_process);

        let later_process = bus::open_store(&dir).unwrap();
        assert_eq!(
            drain_route_held_mail(&dir, &registry, &later_process, "acpx-parent"),
            0
        );
        assert_eq!(std::fs::read_to_string(&calls).unwrap(), "xx");
        match previous {
            Some(value) => std::env::set_var("BOOP_ACPX_BIN", value),
            None => std::env::remove_var("BOOP_ACPX_BIN"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (failure mode 14, rail 2). The one gate every door path calls.
    /// Floor 1 with one push in the ledger: the gate trips once and writes one
    /// blowout row; every later call inside the cool-off answers cool-off and
    /// writes nothing.
    #[test]
    fn the_door_gate_trips_once_then_only_reports_the_cool_off() {
        let (dir, store) = burst_fixture("gate", 0, &["g-one"]);
        let routes = bus::read_routes(&dir).unwrap();
        let budget = budget(60_000, 60_000, 1);
        let now = boop_harness::live::now_ms();
        assert!(
            door_gate(&store, "claude-gate", &routes, "g-one", &budget, now)
                .unwrap()
                .is_none()
        );
        Landing::new(Rung::Door, "door")
            .record(&store, "m-gate-0", "claude-gate", None)
            .unwrap();
        let tripped = door_gate(&store, "claude-gate", &routes, "g-two", &budget, now)
            .unwrap()
            .expect("the second push crosses a floor of one");
        assert!(matches!(tripped.rung, Rung::CoolOff));
        assert_eq!(store.door_blowouts("claude-gate").unwrap().len(), 1);
        let cooling = door_gate(&store, "claude-gate", &routes, "g-three", &budget, now + 1)
            .unwrap()
            .expect("the cool-off is in force");
        assert!(matches!(cooling.rung, Rung::CoolOff));
        assert_eq!(
            store.door_blowouts("claude-gate").unwrap().len(),
            1,
            "a route already cooling off records no second trip"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// REGRESSION (historical floor override). With an explicit floor of two,
    /// a coordinator with one live lane has allowance two. Two distinct pushes
    /// in a 60-second window are open; the third records one 300-second
    /// cool-off. At the exact expiry, a new two-push burst is open again and
    /// its third distinct body records the next cool-off. Every decision uses
    /// an explicit clock value, so this test has no sleep or wall-clock race.
    #[test]
    fn explicit_clock_reproduces_two_push_cooloff_and_subsequent_unique_burst() {
        let (dir, store) = burst_fixture(
            "explicit-clock",
            1,
            &[
                "clock-one",
                "clock-two",
                "clock-three",
                "clock-four",
                "clock-five",
            ],
        );
        let route = "claude-explicit-clock";
        let routes = bus::read_routes(&dir).unwrap();
        let budget = budget(60_000, 300_000, 2);
        const FIRST: u64 = 1_000_000;
        const FIRST_TRIP: u64 = FIRST + 2_000;
        const SECOND_BURST: u64 = FIRST_TRIP + 300_000;

        let record_push = |id: &str, at_ms: u64| {
            store
                .append_delivery_transition(
                    id,
                    route,
                    Some(HarnessId::Claude),
                    "accepted-by-harness",
                    "door",
                    None,
                    at_ms,
                )
                .unwrap();
        };

        assert_eq!(
            door_verdict(&store, route, &routes, "clock-one", &budget, FIRST).unwrap(),
            DoorVerdict::Open
        );
        record_push("m-explicit-clock-0", FIRST);
        assert_eq!(
            door_verdict(&store, route, &routes, "clock-two", &budget, FIRST + 1_000).unwrap(),
            DoorVerdict::Open
        );
        record_push("m-explicit-clock-1", FIRST + 1_000);

        let first = door_gate(&store, route, &routes, "clock-three", &budget, FIRST_TRIP)
            .unwrap()
            .expect("the third unique push crosses allowance two");
        assert_eq!(first.rung, Rung::CoolOff);
        assert_eq!(first.detail, "2 door pushes in 60s against 2 live connects");
        let first_row = store.latest_door_blowout(route).unwrap().unwrap();
        assert_eq!(
            (
                first_row.at_ms,
                first_row.pushes,
                first_row.budget,
                first_row.window_ms,
                first_row.cooldown_ms,
            ),
            (FIRST_TRIP, 2, 2, 60_000, 300_000)
        );

        assert_eq!(
            door_verdict(
                &store,
                route,
                &routes,
                "clock-three",
                &budget,
                SECOND_BURST - 1,
            )
            .unwrap(),
            DoorVerdict::CoolingOff {
                until_ms: SECOND_BURST
            }
        );
        let still_cooling = door_gate(
            &store,
            route,
            &routes,
            "clock-four",
            &budget,
            SECOND_BURST - 1,
        )
        .unwrap()
        .expect("the route stays cooling until the exact expiry");
        assert_eq!(still_cooling.rung, Rung::CoolOff);
        assert_eq!(store.door_blowouts(route).unwrap().len(), 1);

        assert_eq!(
            door_verdict(&store, route, &routes, "clock-three", &budget, SECOND_BURST).unwrap(),
            DoorVerdict::Open,
            "the cool-off expires at at_ms + cooldown_ms"
        );
        record_push("m-explicit-clock-2", SECOND_BURST);
        assert_eq!(
            door_verdict(
                &store,
                route,
                &routes,
                "clock-four",
                &budget,
                SECOND_BURST + 1_000,
            )
            .unwrap(),
            DoorVerdict::Open
        );
        record_push("m-explicit-clock-3", SECOND_BURST + 1_000);

        let second = door_gate(
            &store,
            route,
            &routes,
            "clock-five",
            &budget,
            SECOND_BURST + 2_000,
        )
        .unwrap()
        .expect("the subsequent third unique push crosses allowance two");
        assert_eq!(second.rung, Rung::CoolOff);
        assert_eq!(
            second.detail,
            "2 door pushes in 60s against 2 live connects"
        );
        let trips = store.door_blowouts(route).unwrap();
        assert_eq!(trips.len(), 2);
        assert_eq!(
            (
                trips[0].at_ms,
                trips[0].pushes,
                trips[1].at_ms,
                trips[1].pushes
            ),
            (SECOND_BURST + 2_000, 2, FIRST_TRIP, 2)
        );
        assert_eq!(
            store
                .door_pushes_since(route, SECOND_BURST - 60_000)
                .unwrap(),
            2,
            "the second burst is counted in its own 60-second window"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// REGRESSION (normal unique progress). The default floor admits a burst
    /// of 32 distinct door bodies in one 60-second window. A repeated body is
    /// rejected by the existing anti-loop guard before the aggregate budget
    /// is full. The 33rd distinct body crosses the bounded default and starts
    /// the normal 300-second cool-off. All decisions use an explicit clock.
    #[test]
    fn default_floor_admits_unique_progress_and_bounds_the_33rd_push() {
        let bodies = (0..33)
            .map(|index| format!("unique-progress-{index}"))
            .chain(std::iter::once("unique-progress-0".to_owned()))
            .collect::<Vec<_>>();
        let body_refs = bodies.iter().map(String::as_str).collect::<Vec<_>>();
        let (dir, store) = burst_fixture("default-policy", 1, &body_refs);
        let route = "claude-default-policy";
        let routes = bus::read_routes(&dir).unwrap();
        let budget = DoorBudget::default();
        const BASE: u64 = 2_000_000;

        assert_eq!(budget.floor, 32);
        assert_eq!(budget.allowance(route, &routes), 32);

        let record_push = |index: usize, at_ms: u64| {
            store
                .append_delivery_transition(
                    format!("m-default-policy-{index}").as_str(),
                    route,
                    Some(HarnessId::Claude),
                    "accepted-by-harness",
                    "door",
                    None,
                    at_ms,
                )
                .unwrap();
        };

        for index in 0..8 {
            let at_ms = BASE + index as u64 * 1_000;
            assert_eq!(
                door_verdict(&store, route, &routes, &bodies[index], &budget, at_ms).unwrap(),
                DoorVerdict::Open
            );
            record_push(index, at_ms);
        }

        let duplicate =
            door_verdict(&store, route, &routes, &bodies[0], &budget, BASE + 8_000).unwrap();
        match duplicate {
            DoorVerdict::Blowout {
                pushes,
                budget: allowed,
                why,
            } => {
                assert_eq!((pushes, allowed), (8, 32));
                assert!(why.contains("same body"), "{why}");
            }
            other => panic!("duplicate body was not rejected: {other:?}"),
        }

        for index in 8..32 {
            let at_ms = BASE + index as u64 * 1_000;
            assert_eq!(
                door_verdict(&store, route, &routes, &bodies[index], &budget, at_ms).unwrap(),
                DoorVerdict::Open
            );
            record_push(index, at_ms);
        }

        let unique =
            door_verdict(&store, route, &routes, &bodies[32], &budget, BASE + 32_000).unwrap();
        assert!(matches!(
            unique,
            DoorVerdict::Blowout {
                pushes: 32,
                budget: 32,
                ..
            }
        ));
        let bounded = door_gate(&store, route, &routes, &bodies[32], &budget, BASE + 32_000)
            .unwrap()
            .expect("the 33rd distinct body crosses the default bound");
        assert_eq!(bounded.rung, Rung::CoolOff);
        assert_eq!(
            bounded.detail,
            "32 door pushes in 60s against 32 live connects"
        );
        let trip = store.latest_door_blowout(route).unwrap().unwrap();
        assert_eq!(
            (trip.at_ms, trip.pushes, trip.budget),
            (BASE + 32_000, 32, 32)
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    fn budget(window_ms: u64, cooldown_ms: u64, floor: usize) -> DoorBudget {
        DoorBudget {
            window: Duration::from_millis(window_ms),
            cooldown: Duration::from_millis(cooldown_ms),
            floor,
        }
    }

    /// A coordinator route bound to the fake claude session, with `lanes`
    /// child lane routes naming it as parent, and `bodies` held rows. The rows
    /// wear `request`: the budget is about how many bodies a door may take, and
    /// a progress kind never reaches the door to be counted.
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
            lane.kind = "lane".into();
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
                    kind: "request".into(),
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

    /// One held row of `kind` for a coordinator route bound to the fake door.
    fn kind_fixture(tag: &str, kind: &str) -> (PathBuf, Store, Message) {
        let (dir, store) = burst_fixture(tag, 0, &[]);
        let message = Message {
            id: format!("m-{tag}-row"),
            from: format!("{tag}-lane-0"),
            to: format!("claude-{tag}"),
            from_timestamp: "2026-09-07T00:00:00Z".to_owned(),
            to_timestamp: None,
            kind: kind.into(),
            reply_to: None,
            body: format!("{tag} {kind} row"),
            r#ref: None,
            rc: None,
            detail: None,
        };
        bus::append(&dir, "bus", &message).unwrap();
        (dir, store, message)
    }

    fn land_one(dir: &Path, store: &Store, message: &Message, budget: &DoorBudget) -> Landing {
        let registry = Registry::with(vec![Box::new(FakeClaude)]);
        let routes = bus::read_routes(dir).unwrap();
        deliver_hail_budgeted(&registry, store, &routes, message, &NoPane, budget).unwrap()
    }

    /// RECEIPT (2026-09-07). A lane's end row walks the ladder like a hail and
    /// leaves through the door of a live route; the fake claude door queues it.
    #[test]
    fn a_lane_end_row_takes_the_door_of_a_live_route() {
        for (tag, kind) in [
            ("endresult", "result"),
            ("endexit", "exited_without_completion"),
        ] {
            let (dir, store, message) = kind_fixture(tag, kind);
            let landing = land_one(&dir, &store, &message, &budget(60_000, 60_000, 10));
            assert_eq!(landing.rung, Rung::DoorQueue, "a {kind} row takes the door");
            assert_eq!(landing.rung.state(), DeliveryState::HeldForTurnBoundary);
            assert!(landing.rung.carried_the_body());
            assert!(
                door_bodies().iter().any(|body| body == &message.body),
                "{kind} never reached the door"
            );
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// RECEIPT (supervisor-rows-off-the-door). A progress row stays in the
    /// mailbox whatever the route: the 2026-09-05 flood must not return.
    #[test]
    fn a_lane_progress_row_stays_off_the_door() {
        for (tag, kind) in [("progyield", "yield"), ("progrewound", "head_rewound")] {
            let (dir, store, message) = kind_fixture(tag, kind);
            let landing = land_one(&dir, &store, &message, &budget(60_000, 60_000, 10));
            assert_eq!(
                landing.rung,
                Rung::MailboxOnly,
                "a {kind} row stops at the mailbox"
            );
            assert_eq!(landing.detail, format!("{kind} row; no door"));
            assert!(
                !door_bodies().iter().any(|body| body == &message.body),
                "{kind} opened a door"
            );
            assert_eq!(bus::held_messages(&store, &message.to).unwrap().len(), 1);
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// RECEIPT (2026-09-07). An end row is a hail now, so it answers to the
    /// door budget: a blown budget cools it off rather than mailboxing it.
    #[test]
    fn an_end_row_under_a_blown_budget_cools_off() {
        let (dir, store, message) = kind_fixture("endbudget", "result");
        Landing::new(Rung::Door, "door")
            .record(&store, "m-endbudget-earlier", "claude-endbudget", None)
            .unwrap();
        let landing = land_one(&dir, &store, &message, &budget(60_000, 60_000, 1));
        assert_eq!(landing.rung, Rung::CoolOff, "{landing:?}");
        assert_eq!(landing.rung.state(), DeliveryState::CooledOff);
        assert_eq!(store.door_blowouts("claude-endbudget").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
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
        let taken: Vec<_> = door_bodies()
            .into_iter()
            .filter(|body| body.starts_with("b-"))
            .collect();
        assert_eq!(taken, ["b-one", "b-two"]);

        let trips = store.door_blowouts("claude-burst").unwrap();
        assert_eq!(trips.len(), 1, "{trips:?}");
        assert_eq!((trips[0].pushes, trips[0].budget), (2, 2));
        assert!(
            trips[0]
                .why
                .contains("2 door pushes in 60s against 2 live connects"),
            "{}",
            trips[0].why
        );

        assert_eq!(
            transitions(&store, "m-burst-2"),
            [
                ("appended".to_owned(), "mailbox".to_owned()),
                ("cooled-off".to_owned(), trips[0].why.clone()),
            ]
        );
        let untouched = [("appended".to_owned(), "mailbox".to_owned())];
        for later in ["m-burst-3", "m-burst-4", "m-burst-5"] {
            assert_eq!(
                transitions(&store, later),
                untouched,
                "{later} was touched past the trip"
            );
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
        assert_eq!(
            before, after,
            "a cooling route writes no transition per tick"
        );
        assert_eq!(store.door_blowouts("claude-burst").unwrap().len(), 1);
        assert_eq!(
            door_bodies()
                .iter()
                .filter(|body| body.starts_with("b-"))
                .count(),
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
        let taken: Vec<_> = door_bodies()
            .into_iter()
            .filter(|body| body.starts_with("d-"))
            .collect();
        assert_eq!(taken, ["d-1", "d-2", "d-3", "d-4", "d-5"]);
        assert_eq!(store.door_blowouts("claude-drip").unwrap().len(), 2);
        assert!(bus::held_messages(&store, "claude-drip")
            .unwrap()
            .is_empty());
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
            door_bodies()
                .iter()
                .filter(|body| body.as_str() == "r-same")
                .count(),
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
    /// times. The captured rows were `result`s; this one wears `request`,
    /// because a `result` no longer reaches the door at all and the rail under
    /// test is the requeue, not the kind.
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
            kind: "request".into(),
            reply_to: None,
            body: "already in front of you".to_owned(),
            r#ref: None,
            rc: None,
            detail: None,
        };
        bus::append(&dir, "bus", &message).unwrap();
        let store = bus::open_store(&dir).unwrap();
        for (outcome, detail) in [
            ("appended", "mailbox"),
            ("held-for-turn-boundary", "door queue"),
        ] {
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
        assert_eq!(
            drain_route_held_mail(&dir, &registry, &store, "claude-old"),
            0
        );
        assert!(
            !door_bodies()
                .iter()
                .any(|body| body == "already in front of you"),
            "the door was handed a row it already holds"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (head-rewound-door-retry). The ladder stamps the row, not the
    /// caller. The supervisor's parent hail calls `deliver_hail` and nothing
    /// else, so a door-queue landing that left `to_timestamp` open produced a
    /// row no reader owned: `held_messages` drops it because its ledger names
    /// a door, and `boop wait --me` drops it because its ledger says landed.
    #[test]
    fn a_door_queue_landing_stamps_the_row_at_the_ladder() {
        let (dir, store) = burst_fixture("stamp", 0, &["stamp me at the ladder"]);
        let registry = Registry::with(vec![Box::new(FakeClaude)]);
        let routes = bus::read_routes(&dir).unwrap();
        let message = bus::messages_in(&store)
            .unwrap()
            .into_iter()
            .find(|row| row.id == "m-stamp-0")
            .unwrap();

        let landing = deliver_hail_budgeted(
            &registry,
            &store,
            &routes,
            &message,
            &NoPane,
            &budget(60_000, 60_000, 10),
        )
        .unwrap();

        assert_eq!(landing.rung, Rung::DoorQueue);
        let taken = bus::messages_in(&store)
            .unwrap()
            .into_iter()
            .find(|row| row.id == "m-stamp-0")
            .unwrap();
        assert!(
            taken.to_timestamp.is_some(),
            "the rung carried the body, so the row is history"
        );
        assert!(
            bus::held_messages(&store, "claude-stamp")
                .unwrap()
                .is_empty(),
            "nothing re-pushes a row the door holds"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (head-rewound-door-retry). The gate reads the ledger by
    /// `detail`, so the shape a pre-fix binary wrote every 5s
    /// (`held-for-turn-boundary` / `door queue`) counts as a door push. The
    /// live store held 357 of them for one message on 2026-09-05; the current
    /// gate trips on the second push of that body rather than passing it.
    #[test]
    fn the_gate_counts_a_pre_fix_door_queue_row() {
        let (dir, store) = burst_fixture("prefix", 0, &["the same body twice"]);
        let routes = bus::read_routes(&dir).unwrap();
        let budget = budget(60_000, 60_000, 10);
        let now = boop_harness::live::now_ms();
        store
            .append_delivery_transition(
                "m-prefix-0",
                "claude-prefix",
                Some(HarnessId::Claude),
                "held-for-turn-boundary",
                "door queue",
                None,
                now,
            )
            .unwrap();

        assert_eq!(
            store
                .door_pushes_since("claude-prefix", now - 60_000)
                .unwrap(),
            1,
            "the pre-fix outcome word does not hide the push"
        );
        let tripped = door_gate(
            &store,
            "claude-prefix",
            &routes,
            "the same body twice",
            &budget,
            now,
        )
        .unwrap()
        .expect("the same body inside the window is a replay");
        assert!(matches!(tripped.rung, Rung::CoolOff));
        assert!(store
            .latest_door_blowout("claude-prefix")
            .unwrap()
            .unwrap()
            .why
            .contains("same body"));
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
                kind: "coordinator".into(),
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
            "queued m-1 from coordinator -> claude-77 in the claude door; it reads it at its next turn boundary"
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
                DeliveryState::HeldForTurnBoundary.as_str().to_owned()
            ],
            "{history:#?}"
        );
        assert!(
            !states.iter().any(|state| state.contains("hook-inbox")),
            "no hook rung was walked: {states:?}"
        );
        assert!(store.delivery_accepted("m-claude-77", "claude-77").unwrap());
        let retry =
            deliver_hail_with(&registry, &store, &routes, &message("claude-77"), &NoPane).unwrap();
        assert_eq!(retry.rung, Rung::AlreadyAccepted);
        assert_eq!(
            store.delivery_rows("m-claude-77").unwrap().len(),
            2,
            "queue admission suppresses a second door push"
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

    /// One held commit row addressed at the fake claude coordinator route
    /// `claude-{tag}`, from lane `{tag}-lane-0`.
    fn commit_fixture(tag: &str, body: &str, detail: &str) -> (PathBuf, Store, Message) {
        let (dir, store) = burst_fixture(tag, 0, &[]);
        let message = Message {
            id: format!("m-{tag}-commit"),
            from: format!("{tag}-lane-0"),
            to: format!("claude-{tag}"),
            from_timestamp: "2026-09-11T00:00:00Z".to_owned(),
            to_timestamp: None,
            kind: "commit".into(),
            reply_to: None,
            body: body.to_owned(),
            r#ref: None,
            rc: None,
            detail: Some(detail.to_owned()),
        };
        bus::append(&dir, "bus", &message).unwrap();
        (dir, store, message)
    }

    /// The body a wip commit row carries, with `head` as its new sha.
    fn commit_row_body(head: &str) -> String {
        format!(
            "commit test-lane aaa..{head} n=1 status=wip subject=\"x\" dirty=0\n review: git -C /w log -p aaa..{head}"
        )
    }

    /// Why: a wip commit row to a coordinator route takes the door and the
    /// push is recorded once for (lane, subscriber, head).
    #[test]
    fn a_wip_commit_row_pushes_through_a_coordinator_door_once() {
        let body = commit_row_body("bbb");
        let (dir, store, message) = commit_fixture("commdoor", &body, "wip");
        let landing = land_one(&dir, &store, &message, &budget(60_000, 60_000, 10));
        assert_eq!(landing.rung, Rung::DoorQueue, "{landing:?}");
        assert!(landing.rung.carried_the_body());
        assert!(store
            .commit_push_exists("commdoor-lane-0", "claude-commdoor", "bbb")
            .unwrap());
        assert!(
            door_bodies().iter().any(|body| body.contains("aaa..bbb")),
            "the commit row never reached the door"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Why: a lane parent is delivered at its own turn boundary by its
    /// supervisor, so a commit row for it stays in the mailbox.
    #[test]
    fn a_commit_row_to_a_lane_parent_stays_in_the_mailbox() {
        let dir = std::env::temp_dir().join(format!("boop-commit-lane-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut route = unbound_route(&dir);
        route.kind = "lane".into();
        bus::write_route(&dir, "parent-lane", &route).unwrap();
        let store = bus::open_store(&dir).unwrap();
        let message = Message {
            id: "m-lane-commit".to_owned(),
            from: "child-lane".to_owned(),
            to: "parent-lane".to_owned(),
            from_timestamp: "2026-09-11T00:00:00Z".to_owned(),
            to_timestamp: None,
            kind: "commit".into(),
            reply_to: None,
            body: commit_row_body("bbb"),
            r#ref: None,
            rc: None,
            detail: Some("wip".to_owned()),
        };
        bus::append(&dir, "bus", &message).unwrap();
        let landing = land_one(&dir, &store, &message, &budget(60_000, 60_000, 10));
        assert_eq!(landing.rung, Rung::MailboxOnly, "{landing:?}");
        assert_eq!(landing.detail, "commit row; subscriber reads the mailbox");
        assert!(!store
            .commit_push_exists("child-lane", "parent-lane", "bbb")
            .unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Why: one push per (lane, subscriber, head), so a second row with the
    /// same head is held rather than offered to the door again.
    #[test]
    fn a_commit_head_already_pushed_is_held_on_a_second_row() {
        let body = commit_row_body("bbb");
        let (dir, store, first) = commit_fixture("commdedupe", &body, "wip");
        let landing = land_one(&dir, &store, &first, &budget(60_000, 60_000, 10));
        assert_eq!(landing.rung, Rung::DoorQueue, "{landing:?}");
        let second = Message {
            id: "m-commdedupe-second".to_owned(),
            ..first.clone()
        };
        bus::append(&dir, "bus", &second).unwrap();
        let landing = land_one(&dir, &store, &second, &budget(60_000, 60_000, 10));
        assert_eq!(landing.rung, Rung::MailboxOnly, "{landing:?}");
        assert_eq!(
            landing.detail,
            "commit bbb already pushed to claude-commdedupe"
        );
        let (_, rows) = store
            .passthrough("SELECT COUNT(*) AS n FROM agent_commit_push")
            .unwrap();
        assert_eq!(rows[0]["n"].as_i64(), Some(1));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Why: a `'*'` mailbox subscription for the parent beats the coordinator
    /// door default, so the commit stays in the mailbox.
    #[test]
    fn a_wildcard_mailbox_subscription_keeps_the_parents_commit_held() {
        let (dir, store, message) = commit_fixture("commwild", &commit_row_body("bbb"), "wip");
        store
            .set_commit_subscription(&boop_store::ident::CommitSubscriptionRow {
                subscriber: "claude-commwild".to_owned(),
                lane: "*".to_owned(),
                mode: "mailbox".to_owned(),
                created_at: "2026-09-11T00:00:00Z".to_owned(),
            })
            .unwrap();
        let landing = land_one(&dir, &store, &message, &budget(60_000, 60_000, 10));
        assert_eq!(landing.rung, Rung::MailboxOnly, "{landing:?}");
        assert_eq!(landing.detail, "commit row; subscriber reads the mailbox");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Why: a done commit is the turn's terminal event, so its commit row stays
    /// in the mailbox and the result row carries the review handle.
    #[test]
    fn a_done_commit_row_stays_in_the_mailbox() {
        let (dir, store, message) = commit_fixture("commdone", &commit_row_body("bbb"), "done");
        let landing = land_one(&dir, &store, &message, &budget(60_000, 60_000, 10));
        assert_eq!(landing.rung, Rung::MailboxOnly, "{landing:?}");
        assert!(landing.detail.contains("done"), "{}", landing.detail);
        assert!(!store
            .commit_push_exists("commdone-lane-0", "claude-commdone", "bbb")
            .unwrap());
        let _ = std::fs::remove_dir_all(dir);
    }
}
