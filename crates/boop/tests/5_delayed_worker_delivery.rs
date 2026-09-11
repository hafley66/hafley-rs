//! Delayed worker delivery: a worker's final hail and its completion reach a
//! live Codex coordinator through the door, but the transport actually chosen
//! decides whether they can be read inside the active coordinator turn.
//!
//! The routing contract splits at `boop-proc/src/deliver.rs` `land`:
//!   * a supervisor progress row (`yield`, `head_rewound`, `retrying`) is
//!     `MailboxOnly`;
//!   * a lane end row (`result`, `exited_without_completion`) and a hail take
//!     the door like any other send.
//!
//! This file is the E2E half of the transport readiness check. It proves the
//! routing and the ledger stamp, and it records a timestamped evidence trail.
//! It deliberately does not claim to observe the Codex transport itself: on the
//! pinned base `CodexDoor::deliver` has no injection seam (it shells to
//! `codex queue` directly), so the actual RPC method is not observable from an
//! integration test. That stage is reported as `unknown`, and the pinned base's
//! source is the only evidence for it. See
//! `crates/boop/docs/5_delayed-worker-delivery-reproduction.md`.
//!
//! No model runs, no live session is touched, and no sleep is a timing oracle.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use boop::bus::{Message, Route};
use boop::door::{Delivered, Door, IdleNotice};
use boop::harness::{
    Capabilities, Harness, HarnessId, LanePolicy, MailPolicy, NativeBackendSupport,
    NativeSettingsSupport, ReadChunk, SessionRef, VariantSupport,
};
use boop::live::{DoorAddress, LiveSession, LiveSessionScope, LiveSessions, LiveStatus};
use boop::mail::{deliver_hail_budgeted, DoorBudget, PanePaster, Rung};
use boop::registry::Registry;
use boop::Store;

/// A door harness that takes mail and records the body it was handed. The
/// recording is the seam that lets the test name the transport, and it is the
/// same shape the real Codex door is reached through.
static DOOR: Capabilities = Capabilities {
    bans_plan_family_models: false,
    lanes: LanePolicy::Allowed,
    variant: VariantSupport::None,
    mail: MailPolicy::Door,
    image_paste_keys: None,
    interrupt_keys: None,
    native_tui_projector: false,
    wrapper_owns_alternate_screen: false,
    native_backend: NativeBackendSupport::Unsupported,
    native_settings: NativeSettingsSupport::Unsupported("fixture"),
};

struct OneSession {
    session_id: String,
    socket: PathBuf,
}

impl LiveSessions for OneSession {
    fn live_sessions(&self) -> Result<Vec<LiveSession>> {
        Ok(vec![LiveSession {
            harness: HarnessId::Codex,
            session_id: self.session_id.clone(),
            pid: Some(std::process::id()),
            cwd: None,
            tmux_pane: None,
            status: LiveStatus::Unknown,
            door: DoorAddress::AppServer {
                socket: self.socket.clone(),
                thread: self.session_id.clone(),
            },
            observed_ms: 0,
            started_ms: None,
            scope: LiveSessionScope::Root,
            parent_session: None,
        }])
    }
}

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

struct Probe {
    live: OneSession,
    door: Recorder,
}

impl Harness for Probe {
    fn id(&self) -> HarnessId {
        HarnessId::Codex
    }

    fn mock_tui_launch(
        &self,
        _: &boop::harness::mock_tui::MockTuiContext<'_>,
    ) -> anyhow::Result<boop::harness::mock_tui::MockTuiLaunch> {
        anyhow::bail!("fixture harness has no mock launch")
    }

    fn capabilities(&self) -> &'static Capabilities {
        &DOOR
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

struct NoPane;

impl PanePaster for NoPane {
    fn paste(&self, _pane: &str, _notice: &str) -> Option<String> {
        None
    }
}

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "boop-delayed-delivery-{}-{label}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn route(thread: &str, socket: &Path) -> Route {
    Route {
        kind: "coordinator".into(),
        harness: Some(HarnessId::Codex),
        tmux: None,
        cwd: None,
        model: None,
        mode: None,
        session_id: Some(thread.into()),
        source_path: None,
        parent: None,
        goal: None,
        registered_at: None,
        base_sha: None,
        worktree_dir: None,
        app_server_socket: Some(socket.display().to_string()),
    }
}

fn message(id: &str, from: &str, to: &str, kind: &str, body: &str, at: &str) -> Message {
    Message {
        id: id.into(),
        from: from.into(),
        to: to.into(),
        from_timestamp: at.into(),
        to_timestamp: None,
        kind: kind.into(),
        reply_to: None,
        body: body.into(),
        r#ref: None,
        rc: None,
        detail: None,
    }
}

/// RECEIPT. A completion and a final hail to a live Codex coordinator take the
/// door and are stamped `accepted-by-harness`; a supervisor progress row to the
/// same route and over the same body is `MailboxOnly`. The evidence trail marks
/// the transport and model consumption as unobservable rather than inferring
/// either from the acknowledgement.
#[test]
fn worker_completion_and_hail_take_the_door_while_progress_stays_in_the_mailbox() {
    let dir = temp_dir("routing");
    let store = Store::open(dir.join("boop.db")).unwrap();
    let thread = "01a07c48-delayed-thread";
    let socket = dir.join("app-server-control.sock");
    let log = dir.join("door.log");
    let registry = Registry::with(vec![Box::new(Probe {
        live: OneSession {
            session_id: thread.into(),
            socket: socket.clone(),
        },
        door: Recorder { log: log.clone() },
    })]);
    let routes = BTreeMap::from([("codex-coord".to_owned(), route(thread, &socket))]);
    let budget = DoorBudget {
        window: Duration::ZERO,
        cooldown: Duration::ZERO,
        floor: 100,
    };

    // The two rows a worker's ending produces, in the observed order and with
    // the observation run's creation stamps.
    let cases = [
        (
            "m-complete",
            "result",
            "lane feature-f41-ground-chart done rc=0",
            "2026-09-10T14:00:18.692426Z",
        ),
        (
            "m-hail",
            "request",
            "[boop m-hail from feature-f41] final hail: charts shipped",
            "2026-09-10T14:00:16.030404Z",
        ),
    ];

    let mut timeline = Vec::new();
    for (id, kind, body, created) in cases {
        let msg = message(
            id,
            "feature-f41-ground-chart",
            "codex-coord",
            kind,
            body,
            created,
        );
        let started = Instant::now();
        let landing =
            deliver_hail_budgeted(&registry, &store, &routes, &msg, &NoPane, &budget).unwrap();
        let rows = store.delivery_rows(id).unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.outcome.as_str())
                .collect::<Vec<_>>(),
            ["appended", "accepted-by-harness"],
            "{id} ({kind}) must append then land on the door"
        );
        assert_eq!(
            rows.iter().map(|row| row.sequence).collect::<Vec<_>>(),
            [1, 2],
            "{id} records the append then the rung"
        );
        assert_eq!(rows.last().unwrap().harness.as_deref(), Some("codex"));
        assert_eq!(landing.rung, Rung::Door, "{id} ({kind})");
        assert_eq!(landing.detail, "door");

        // The mailbox stamp in UTC ISO8601 and the monotonic elapsed in
        // microseconds. The door receipt proves acceptance, never consumption.
        timeline.push(serde_json::json!({
            "stage": "dispatch-attempt",
            "message_id": id,
            "kind": kind,
            "created_at_logical": created,
            "mailbox_stamp_ms": rows.last().unwrap().at_ms,
            "mailbox_stamp_iso": iso8601(rows.last().unwrap().at_ms),
            "rung": landing.rung.as_str(),
            "detail": landing.detail,
            "selected_harness": "codex",
            "selected_door": "recorder seam",
            "rpc_method": "unknown (Recorder::deliver is the test seam)",
            "rpc_acknowledgement": "unknown (the recorder returns Delivered::Injected)",
            "monotonic_elapsed_us": started.elapsed().as_micros() as u64,
            "model_consumed": "unknown",
        }));
    }

    // The same route, the same kind of body, but a supervisor progress row.
    let progress = message(
        "m-yield",
        "feature-f41-ground-chart",
        "codex-coord",
        "yield",
        "lane feature-f41 still working",
        "2026-09-10T13:59:00.000000Z",
    );
    let landing =
        deliver_hail_budgeted(&registry, &store, &routes, &progress, &NoPane, &budget).unwrap();
    let rows = store.delivery_rows("m-yield").unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row.outcome.as_str())
            .collect::<Vec<_>>(),
        ["appended", "held-in-mailbox"],
        "a yield row never opens the door"
    );
    assert_eq!(landing.rung, Rung::MailboxOnly);
    assert_eq!(landing.detail, "yield row; no door");

    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        format!(
            "{thread} <- [boop m-hail from feature-f41-ground-chart] [boop m-hail from feature-f41] final hail: charts shipped"
        ),
        "the last delivery reached the door, sender named (the recorder keeps the last body)"
    );

    for row in &timeline {
        println!("EVIDENCE {row}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

fn iso8601(ms: i64) -> String {
    use time::format_description::well_known::Rfc3339;

    time::OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .unwrap()
        .format(&Rfc3339)
        .unwrap()
}
