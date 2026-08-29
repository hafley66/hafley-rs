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
use boop::mail::{deliver_hail_with, PanePaster, Rung};
use boop::registry::Registry;
use boop::Store;

static DOOR: Capabilities = Capabilities {
    bans_plan_family_models: false,
    lanes: LanePolicy::Allowed,
    variant: VariantSupport::None,
    mail: MailPolicy::Door,
    native_tui_projector: false,
    wrapper_owns_alternate_screen: false,
};

static KEYSTROKES: Capabilities = Capabilities {
    bans_plan_family_models: false,
    lanes: LanePolicy::Allowed,
    variant: VariantSupport::None,
    mail: MailPolicy::Keystrokes,
    native_tui_projector: false,
    wrapper_owns_alternate_screen: false,
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

/// A pane that takes nothing. Rung 4 must never touch a real terminal from a
/// test, and a dead pane is what most routes have anyway.
struct NoPane;

impl PanePaster for NoPane {
    fn paste(&self, _pane: &str, _notice: &str) -> Option<String> {
        None
    }
}

/// A pane that records what it was handed, for the one test that asserts the
/// paste rung.
#[derive(Default)]
struct RecordingPane {
    pasted: std::sync::Mutex<Vec<(String, String)>>,
}

impl PanePaster for RecordingPane {
    fn paste(&self, pane: &str, notice: &str) -> Option<String> {
        self.pasted
            .lock()
            .unwrap()
            .push((pane.to_owned(), notice.to_owned()));
        Some(pane.to_owned())
    }
}

fn routes(name: &str, route: Route) -> BTreeMap<String, Route> {
    BTreeMap::from([(name.to_owned(), route)])
}

/// RECEIPT. A hail to a Door harness reaches that harness's own door and
/// leaves its append and acceptance transitions in order.
#[test]
fn a_door_harness_takes_the_body_and_leaves_one_delivery_row() {
    let dir = temp_dir("door");
    let store = store(&dir);
    let registry = Registry::with(vec![Box::new(echo(HarnessId::Kimi, &DOOR, &dir))]);
    let routes = routes("tui", route(HarnessId::Kimi, Some("projects:@1.%1"), None));
    let message = message("m-door", "tui", "ping through the door");

    let landing = deliver_hail_with(&registry, &store, &routes, &message, &NoPane).unwrap();
    assert_eq!(landing.rung, Rung::Door);
    assert_eq!(
        std::fs::read_to_string(dir.join("door.log")).unwrap(),
        "live-1 <- ping through the door"
    );

    let rows = store.delivery_rows("m-door").unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| (
                row.sequence,
                row.route.as_str(),
                row.harness.as_deref(),
                row.outcome.as_str(),
                row.detail.as_str()
            ))
            .collect::<Vec<_>>(),
        [
            (1, "tui", Some("kimi"), "appended", "mailbox"),
            (2, "tui", Some("kimi"), "accepted-by-harness", "door"),
        ]
    );

    let live = store.live_row("live-1").unwrap().unwrap();
    assert_eq!(live.door_kind.as_deref(), Some("unix-socket"));
    assert_eq!(
        live.door_addr.as_deref(),
        Some(dir.join("door.sock").display().to_string().as_str()),
        "the door address is projected; the token never is"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// RECEIPT. A harness that still declares keystrokes has no door to try, so
/// the ladder walks past it and the row is held where a retry finds it.
#[test]
fn a_keystrokes_harness_falls_to_the_mailbox_rung() {
    let dir = temp_dir("keys");
    let store = store(&dir);
    let registry = Registry::with(vec![Box::new(echo(HarnessId::Codex, &KEYSTROKES, &dir))]);
    let routes = routes("typed", route(HarnessId::Codex, Some("%1"), None));
    let message = message("m-keys", "typed", "never typed");

    let landing = deliver_hail_with(&registry, &store, &routes, &message, &NoPane).unwrap();
    assert_eq!(landing.rung, Rung::Mailbox);
    assert_eq!(landing.detail, "harness takes no door mail");
    assert!(!dir.join("door.log").exists(), "no door was opened");

    let rows = store.delivery_rows("m-keys").unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| (
                row.outcome.as_str(),
                row.detail.as_str(),
                row.harness.as_deref()
            ))
            .collect::<Vec<_>>(),
        [
            ("appended", "mailbox", Some("codex")),
            (
                "held-in-mailbox",
                "harness takes no door mail",
                Some("codex")
            ),
        ]
    );
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

    let landing = deliver_hail_with(&registry, &store, &routes, &message, &NoPane).unwrap();
    assert_eq!(landing.rung, Rung::Door);
    assert_eq!(
        std::fs::read_to_string(dir.join("door.log")).unwrap(),
        "stored-1 <- through the projection"
    );
    assert_eq!(
        store
            .delivery_rows("m-fallback")
            .unwrap()
            .last()
            .map(|row| row.outcome.as_str()),
        Some("accepted-by-harness")
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// RECEIPT. A name with no route still lands: the row is held in the mailbox
/// and the reason rides on the transition, so a retry knows what to re-try.
#[test]
fn a_hail_with_no_route_is_held_in_the_mailbox() {
    let dir = temp_dir("routeless");
    let store = store(&dir);
    let registry = Registry::with(vec![Box::new(echo(HarnessId::Kimi, &DOOR, &dir))]);
    let message = message("m-none", "nobody", "into the void");

    let landing =
        deliver_hail_with(&registry, &store, &BTreeMap::new(), &message, &NoPane).unwrap();
    assert_eq!(landing.rung, Rung::Mailbox);
    assert_eq!(landing.detail, "no registry route for nobody");
    let rows = store.delivery_rows("m-none").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.last().unwrap().harness, None);
    assert_eq!(rows.last().unwrap().outcome, "held-in-mailbox");
    let _ = std::fs::remove_dir_all(dir);
}

/// RECEIPT. A route that names no harness has no door to try, so the ladder
/// falls to the mailbox rung rather than delivering through a guessed default.
#[test]
fn a_route_with_no_harness_falls_to_the_mailbox_rung() {
    let dir = temp_dir("harnessless");
    let store = store(&dir);
    let registry = Registry::with(vec![Box::new(echo(HarnessId::Kimi, &DOOR, &dir))]);
    let mut native = route(HarnessId::Kimi, Some("%1"), None);
    native.kind = "native".into();
    native.harness = None;
    let routes = routes("native-worker", native);
    let message = message("m-harnessless", "native-worker", "into the void");

    let landing = deliver_hail_with(&registry, &store, &routes, &message, &NoPane).unwrap();
    assert_eq!(landing.rung, Rung::Mailbox);
    assert!(!dir.join("door.log").exists(), "no door was opened");

    let rows = store.delivery_rows("m-harnessless").unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| (
                row.outcome.as_str(),
                row.detail.as_str(),
                row.harness.as_deref()
            ))
            .collect::<Vec<_>>(),
        [
            ("appended", "mailbox", None),
            (
                "held-in-mailbox",
                "route native-worker names no harness",
                None
            ),
        ]
    );
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

    let landing = deliver_hail_with(&registry, &store, &routes, &message, &NoPane).unwrap();
    assert_eq!(landing.rung, Rung::TurnBoundary);
    assert_eq!(landing.detail, "lane supervisor");

    let rows = store.delivery_rows("m-lane").unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| (row.outcome.as_str(), row.detail.as_str()))
            .collect::<Vec<_>>(),
        [
            ("appended", "mailbox"),
            ("held-for-turn-boundary", "lane supervisor"),
        ],
        "a lane row is held by its own supervisor, and the sender is told so"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// RECEIPT (ladder rung 4). A route whose door answered nothing, with no hook
/// installed and a live pane, has the notice pasted into that pane, and the
/// transition names the rung.
#[test]
fn a_route_with_a_live_pane_takes_the_paste_rung() {
    let dir = temp_dir("paste");
    let store = store(&dir);
    let registry = Registry::with(vec![Box::new(echo(HarnessId::Codex, &KEYSTROKES, &dir))]);
    let routes = routes("typed", route(HarnessId::Codex, Some("%1"), None));
    let message = message("m-paste", "typed", "read this");
    let pane = RecordingPane::default();

    let landing = deliver_hail_with(&registry, &store, &routes, &message, &pane).unwrap();
    assert_eq!(landing.rung, Rung::PanePaste);
    let pasted = pane.pasted.lock().unwrap().clone();
    assert_eq!(pasted.len(), 1, "one paste, one pane");
    assert!(
        pasted[0].1.contains("boop inbox drain"),
        "notice: {}",
        pasted[0].1
    );
    assert_eq!(
        store
            .delivery_rows("m-paste")
            .unwrap()
            .last()
            .map(|row| row.outcome.clone()),
        Some("pasted-into-pane".to_owned())
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// RECEIPT (ladder). Every rung records a transition the sender can read back,
/// and none of them is `appended` alone: a row nobody owns is the one failure.
#[test]
fn every_landing_records_a_transition_past_appended() {
    let dir = temp_dir("ladder");
    let store = store(&dir);
    let registry = Registry::with(vec![Box::new(echo(HarnessId::Kimi, &DOOR, &dir))]);
    let mut lane = route(HarnessId::Kimi, None, None);
    lane.kind = "lane".into();
    let cases: [(&str, BTreeMap<String, Route>, &str); 3] = [
        (
            "m-l1",
            routes("tui", route(HarnessId::Kimi, Some("projects:@1.%1"), None)),
            "tui",
        ),
        ("m-l2", routes("feature-lane", lane), "feature-lane"),
        ("m-l3", BTreeMap::new(), "nobody"),
    ];
    for (id, routes, to) in cases {
        let message = message(id, to, "body");
        let landing = deliver_hail_with(&registry, &store, &routes, &message, &NoPane).unwrap();
        assert!(
            landing.state().landed(),
            "{id} landed on {:?}, which is not a landed state",
            landing.rung
        );
        let rows = store.delivery_rows(id).unwrap();
        assert_eq!(rows.first().unwrap().outcome, "appended", "{id}");
        assert_eq!(rows.last().unwrap().outcome, landing.outcome(), "{id}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// RECEIPT (rung selection). A codex route whose door answers nothing is held
/// for its next turn boundary. The paster is never reached: a codex or claude
/// TUI pane takes mail through its door or not at all, and typing at one puts
/// keys in front of whoever is sitting there.
#[test]
fn a_door_harness_with_no_live_session_is_held_and_never_pasted() {
    let dir = temp_dir("codex-door-down");
    let store = store(&dir);
    let mut harness = echo(HarnessId::Codex, &DOOR, &dir);
    harness.live.pane = None;
    let registry = Registry::with(vec![Box::new(harness)]);
    let mut coordinator = route(HarnessId::Codex, Some("%1"), None);
    coordinator.kind = "coordinator".into();
    let routes = routes("codex-0", coordinator);
    let message = message("m-codex", "codex-0", "read this");
    let pane = RecordingPane::default();

    let landing = deliver_hail_with(&registry, &store, &routes, &message, &pane).unwrap();
    assert_eq!(landing.rung, Rung::TurnBoundary);
    assert!(
        landing.detail.contains("no live codex session"),
        "detail: {}",
        landing.detail
    );
    assert!(
        pane.pasted.lock().unwrap().is_empty(),
        "a door harness was pasted into: {:?}",
        pane.pasted.lock().unwrap()
    );
    assert_eq!(
        store
            .delivery_rows("m-codex")
            .unwrap()
            .last()
            .map(|row| row.outcome.clone()),
        Some("held-for-turn-boundary".to_owned())
    );
    let _ = std::fs::remove_dir_all(dir);
}
