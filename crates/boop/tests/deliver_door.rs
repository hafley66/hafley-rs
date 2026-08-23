//! One hail, one door, one `agent_delivery` row. The harness under test is a
//! whole harness: one `impl Harness` registered under a closed enum variant.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use boop::bus::{Message, Route};
use boop::door::{Delivered, Door, IdleNotice};
use boop::harness::{
    Capabilities, Harness, HarnessId, LanePolicy, MailPolicy, ReadChunk, SessionRef, VariantSupport,
};
use boop::live::{DoorAddress, LiveSession, LiveSessions, LiveStatus};
use boop::mail::{deliver_hail, Via};
use boop::registry::Registry;
use boop::Store;

static DOOR: Capabilities = Capabilities {
    bans_plan_family_models: false,
    lanes: LanePolicy::Allowed,
    variant: VariantSupport::None,
    mail: MailPolicy::Door,
    native_tui_projector: false,
};

static KEYSTROKES: Capabilities = Capabilities {
    bans_plan_family_models: false,
    lanes: LanePolicy::Allowed,
    variant: VariantSupport::None,
    mail: MailPolicy::Keystrokes,
    native_tui_projector: false,
};

/// A door that keeps what it was handed. The file is the recorder, so the
/// harness stays the shareable, lock-free value the trait asks for.
struct Recorder {
    log: PathBuf,
}

impl Door for Recorder {
    fn deliver(&self, session: &LiveSession, body: &str) -> Result<Delivered> {
        std::fs::write(&self.log, format!("{} <- {body}", session.session_id))?;
        Ok(Delivered::Injected)
    }

    fn notify_idle(&self, _session: &LiveSession, _timeout: Duration) -> Result<IdleNotice> {
        anyhow::bail!("the recorder door reports no idle signal")
    }
}

/// A harness registry holding exactly one running session.
struct OneSession {
    session_id: String,
    pane: Option<String>,
    socket: PathBuf,
}

impl LiveSessions for OneSession {
    fn live_sessions(&self) -> Result<Vec<LiveSession>> {
        Ok(vec![LiveSession {
            harness: HarnessId::Kimi,
            session_id: self.session_id.clone(),
            pid: Some(std::process::id()),
            cwd: None,
            tmux_pane: self.pane.clone(),
            status: LiveStatus::Idle,
            door: DoorAddress::UnixSocket {
                path: self.socket.clone(),
                token: Some("never-projected".into()),
            },
            observed_ms: 7,
            started_ms: None,
        }])
    }
}

struct Echo {
    live: OneSession,
    door: Recorder,
    capabilities: &'static Capabilities,
    id: HarnessId,
}

impl Harness for Echo {
    fn id(&self) -> HarnessId {
        self.id
    }

    fn capabilities(&self) -> &'static Capabilities {
        self.capabilities
    }

    fn live(&self) -> &dyn LiveSessions {
        &self.live
    }

    fn door(&self) -> &dyn Door {
        &self.door
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

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("boop-deliver-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn store(dir: &std::path::Path) -> Store {
    Store::open(dir.join("boop.db")).unwrap()
}

fn echo(id: HarnessId, capabilities: &'static Capabilities, dir: &std::path::Path) -> Echo {
    Echo {
        live: OneSession {
            session_id: "live-1".into(),
            pane: Some("%1".into()),
            socket: dir.join("door.sock"),
        },
        door: Recorder {
            log: dir.join("door.log"),
        },
        capabilities,
        id,
    }
}

fn route(harness: HarnessId, tmux: Option<&str>, session_id: Option<&str>) -> Route {
    Route {
        kind: "coordinator".into(),
        harness: Some(harness),
        tmux: tmux.map(str::to_owned),
        cwd: None,
        model: None,
        mode: None,
        session_id: session_id.map(str::to_owned),
        source_path: None,
        parent: None,
        goal: None,
        registered_at: None,
        base_sha: None,
        worktree_dir: None,
        app_server_socket: None,
    }
}

fn message(id: &str, to: &str, body: &str) -> Message {
    Message {
        id: id.into(),
        from: "coordinator".into(),
        to: to.into(),
        from_timestamp: "2026-08-22T00:00:00Z".into(),
        to_timestamp: None,
        kind: "request".into(),
        reply_to: None,
        body: body.into(),
        r#ref: None,
        rc: None,
        detail: None,
    }
}

fn routes(name: &str, route: Route) -> BTreeMap<String, Route> {
    BTreeMap::from([(name.to_owned(), route)])
}

/// RECEIPT. A hail to a Door harness reaches that harness's own door, the
/// pane its live registry reports, and leaves exactly one ledger row.
#[test]
fn a_door_harness_takes_the_body_and_leaves_one_delivery_row() {
    let dir = temp_dir("door");
    let store = store(&dir);
    let registry = Registry::with(vec![Box::new(echo(HarnessId::Kimi, &DOOR, &dir))]);
    let routes = routes("tui", route(HarnessId::Kimi, Some("projects:@1.%1"), None));
    let message = message("m-door", "tui", "ping through the door");

    let landing = deliver_hail(&registry, &store, &routes, &message).unwrap();
    assert_eq!(landing.delivered, Delivered::Injected);
    assert_eq!(landing.via, Via::Door);
    assert_eq!(
        std::fs::read_to_string(dir.join("door.log")).unwrap(),
        "live-1 <- ping through the door"
    );

    let rows = store.delivery_rows("m-door").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].route, "tui");
    assert_eq!(rows[0].harness.as_deref(), Some("kimi"));
    assert_eq!(rows[0].outcome, "injected");
    assert_eq!(rows[0].detail, "door");

    let live = store.live_row("live-1").unwrap().unwrap();
    assert_eq!(live.door_kind.as_deref(), Some("unix-socket"));
    assert_eq!(
        live.door_addr.as_deref(),
        Some(dir.join("door.sock").display().to_string().as_str()),
        "the door address is projected; the token never is"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// RECEIPT. A harness that still declares keystrokes gets no delivery at all,
/// and the refusal is a row, not a silent queue.
#[test]
fn a_keystrokes_harness_is_unreachable_and_still_records_a_row() {
    let dir = temp_dir("keys");
    let store = store(&dir);
    let registry = Registry::with(vec![Box::new(echo(HarnessId::Codex, &KEYSTROKES, &dir))]);
    let routes = routes("typed", route(HarnessId::Codex, Some("%1"), None));
    let message = message("m-keys", "typed", "never typed");

    let landing = deliver_hail(&registry, &store, &routes, &message).unwrap();
    assert_eq!(
        landing.delivered,
        Delivered::Unreachable("keystroke delivery retired".into())
    );
    assert!(!dir.join("door.log").exists(), "no door was opened");

    let rows = store.delivery_rows("m-keys").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].outcome, "unreachable");
    assert_eq!(rows[0].detail, "keystroke delivery retired");
    assert_eq!(rows[0].harness.as_deref(), Some("codex"));
    let _ = std::fs::remove_dir_all(dir);
}

/// RECEIPT. A harness whose own registry has gone quiet still addresses the
/// session through the last `agent_live` projection, door included.
#[test]
fn a_door_falls_back_to_the_last_agent_live_row() {
    let dir = temp_dir("fallback");
    let store = store(&dir);
    let mut harness = echo(HarnessId::Kimi, &DOOR, &dir);
    harness.live.pane = None;
    let registry = Registry::with(vec![Box::new(harness)]);
    let routes = routes("gone", route(HarnessId::Kimi, Some("%9"), Some("stored-1")));
    let message = message("m-fallback", "gone", "through the projection");

    store
        .record_status("stored-1", 5, "idle", Some(4242), Some("%9"))
        .unwrap();
    store
        .record_live_door("stored-1", "unix-socket", Some("/tmp/stored.sock"))
        .unwrap();

    let landing = deliver_hail(&registry, &store, &routes, &message).unwrap();
    assert_eq!(landing.delivered, Delivered::Injected);
    assert_eq!(
        std::fs::read_to_string(dir.join("door.log")).unwrap(),
        "stored-1 <- through the projection"
    );
    assert_eq!(
        store.delivery_rows("m-fallback").unwrap()[0].outcome,
        "injected"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// RECEIPT. A name with no route is one unreachable row, so `boop wait` can
/// say why nothing arrived instead of waiting on a message nobody took.
#[test]
fn a_hail_with_no_route_records_its_own_refusal() {
    let dir = temp_dir("routeless");
    let store = store(&dir);
    let registry = Registry::with(vec![Box::new(echo(HarnessId::Kimi, &DOOR, &dir))]);
    let message = message("m-none", "nobody", "into the void");

    let landing = deliver_hail(&registry, &store, &BTreeMap::new(), &message).unwrap();
    assert_eq!(
        landing.delivered,
        Delivered::Unreachable("no registry route for nobody".into())
    );
    let rows = store.delivery_rows("m-none").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].harness, None);
    let _ = std::fs::remove_dir_all(dir);
}

/// RECEIPT. A route that names no harness has no door to try, so the hail is
/// one unreachable row rather than a delivery through a guessed default.
#[test]
fn a_route_with_no_harness_is_unreachable_and_records_it() {
    let dir = temp_dir("harnessless");
    let store = store(&dir);
    let registry = Registry::with(vec![Box::new(echo(HarnessId::Kimi, &DOOR, &dir))]);
    let mut native = route(HarnessId::Kimi, Some("%1"), None);
    native.kind = "native".into();
    native.harness = None;
    let routes = routes("native-worker", native);
    let message = message("m-harnessless", "native-worker", "into the void");

    let landing = deliver_hail(&registry, &store, &routes, &message).unwrap();
    assert_eq!(
        landing.delivered,
        Delivered::Unreachable("route native-worker names no harness".into())
    );
    assert!(!dir.join("door.log").exists(), "no door was opened");

    let rows = store.delivery_rows("m-harnessless").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].outcome, "unreachable");
    assert_eq!(rows[0].detail, "route native-worker names no harness");
    assert_eq!(rows[0].harness, None);
    let _ = std::fs::remove_dir_all(dir);
}

/// RECEIPT. A lane route lands at its own supervisor before any harness is
/// looked up, so a lane written without a harness keeps taking mail.
#[test]
fn a_lane_route_with_no_harness_still_lands_at_its_supervisor() {
    let dir = temp_dir("lane");
    let store = store(&dir);
    let registry = Registry::with(vec![Box::new(echo(HarnessId::Kimi, &DOOR, &dir))]);
    let mut lane = route(HarnessId::Kimi, None, None);
    lane.kind = "lane".into();
    lane.harness = None;
    let routes = routes("feature-lane", lane);
    let message = message("m-lane", "feature-lane", "work this");

    let landing = deliver_hail(&registry, &store, &routes, &message).unwrap();
    assert_eq!(landing.delivered, Delivered::QueuedForTurnBoundary);
    assert_eq!(landing.via, Via::LaneSupervisor);

    let rows = store.delivery_rows("m-lane").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].outcome, "queued-for-turn-boundary");
    assert_eq!(rows[0].detail, "lane supervisor");
    let _ = std::fs::remove_dir_all(dir);
}
