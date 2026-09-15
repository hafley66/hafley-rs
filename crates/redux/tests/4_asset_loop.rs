//! Integration tests for the `1_asset_loop` example: real temporary-file I/O,
//! real Tokio tasks and bounded channels, deterministic channel handshakes.
//! No sleeps, no faked executors, no unit fakes for I/O.

#[path = "../examples/1_asset_loop.rs"]
mod asset_loop;

use asset_loop::{
    AssetEvent, AssetHost, AssetId, BLOB_CAPACITY, Gate, InFlight, MAX_IN_FLIGHT, ReadStatus,
    SettledRequest, ShutdownReport,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use tokio::sync::mpsc;

static UNIQUE: AtomicU64 = AtomicU64::new(0);

/// A unique temporary directory (tempfile-owned, removed on drop) holding one
/// asset file with known bytes.
struct TempAsset {
    _dir: TempDir,
    path: String,
}

impl TempAsset {
    fn new(label: &str, bytes: &[u8]) -> Self {
        let id = UNIQUE.fetch_add(1, Ordering::Relaxed);
        let dir = tempfile::tempdir().expect("unique tempdir");
        let path = dir
            .path()
            .join(format!("redux-asset-loop-{label}-{id}.bin"));
        let path = path.to_string_lossy().into_owned();
        std::fs::write(&path, bytes).expect("write temp asset");
        TempAsset { _dir: dir, path }
    }
}

/// The test side of one worker handshake, in spawn order.
struct GateWatch {
    started: mpsc::Receiver<u64>,
    proceed: mpsc::Sender<()>,
    delivered: mpsc::Receiver<u64>,
}

fn gate_pair() -> (Gate, GateWatch) {
    let (started_tx, started_rx) = mpsc::channel(1);
    let (proceed_tx, proceed_rx) = mpsc::channel(1);
    let (delivered_tx, delivered_rx) = mpsc::channel(1);
    (
        Gate {
            started: started_tx,
            proceed: proceed_rx,
            delivered: delivered_tx,
        },
        GateWatch {
            started: started_rx,
            proceed: proceed_tx,
            delivered: delivered_rx,
        },
    )
}

/// Settle every outstanding request and reap every finished task, bounded so a
/// hang is a panic. Ordering comes from the gate handshakes; this loop only
/// polls a monotone condition and always terminates once workers finish.
fn settle_all(host: &mut AssetHost) {
    for _ in 0..1000 {
        host.tick();
        if host.state().pending.is_empty() && host.unreaped_tasks() == 0 {
            return;
        }
    }
    panic!("pending window or task set never drained");
}

#[test]
fn successful_read_settles_status_and_host_owned_blob() {
    let asset = TempAsset::new("success", b"asset-bytes");
    let (gate, mut watch) = gate_pair();
    let mut host = AssetHost::with_gates(vec![gate]);

    assert_eq!(
        host.request_load(asset.path.clone()),
        asset_loop::Admission::Accepted { request_id: 0 }
    );
    assert_eq!(watch.started.blocking_recv().expect("worker started"), 0);
    watch.proceed.blocking_send(()).expect("release worker");
    assert_eq!(
        watch
            .delivered
            .blocking_recv()
            .expect("completion delivered"),
        0
    );
    settle_all(&mut host);

    assert_eq!(
        host.state().settled,
        Some(SettledRequest {
            request_id: 0,
            generation: 0,
            status: ReadStatus::Succeeded { bytes: 11 },
            asset: Some(AssetId(0)),
        })
    );
    assert_eq!(host.state().settled_count, 1);
    assert_eq!(host.asset(AssetId(0)), Some(b"asset-bytes".as_slice()));
    assert_eq!(host.shutdown(), ShutdownReport { joined: 0 });
}

#[test]
fn missing_file_settles_failure_without_blob() {
    let (gate, mut watch) = gate_pair();
    let mut host = AssetHost::with_gates(vec![gate]);
    let missing = format!(
        "{}/redux-asset-loop-missing-{}",
        std::env::temp_dir().to_string_lossy(),
        UNIQUE.fetch_add(1, Ordering::Relaxed)
    );

    host.request_load(missing);
    assert_eq!(watch.started.blocking_recv().expect("worker started"), 0);
    watch.proceed.blocking_send(()).expect("release worker");
    assert!(watch.delivered.blocking_recv().is_some());
    settle_all(&mut host);

    let settled = host.state().settled.as_ref().expect("settled");
    assert_eq!(settled.asset, None);
    match &settled.status {
        ReadStatus::Failed { message } => {
            assert!(message.contains("os error"), "real OS error: {message}");
        }
        other => panic!("expected failure, got {other:?}"),
    }
    assert_eq!(host.shutdown(), ShutdownReport { joined: 0 });
}

#[test]
fn two_concurrent_same_generation_reads_both_settle() {
    let first = TempAsset::new("con-1", b"first-bytes");
    let second = TempAsset::new("con-2", b"second-bytes");
    let (gate_a, mut watch_a) = gate_pair();
    let (gate_b, mut watch_b) = gate_pair();
    let mut host = AssetHost::with_gates(vec![gate_a, gate_b]);

    assert_eq!(
        host.request_load(first.path.clone()),
        asset_loop::Admission::Accepted { request_id: 0 }
    );
    assert_eq!(
        host.request_load(second.path.clone()),
        asset_loop::Admission::Accepted { request_id: 1 }
    );
    assert_eq!(host.state().generation, 0, "requests share the scope epoch");
    assert_eq!(watch_a.started.blocking_recv().expect("started"), 0);
    assert_eq!(watch_b.started.blocking_recv().expect("started"), 1);
    watch_a.proceed.blocking_send(()).expect("release");
    watch_b.proceed.blocking_send(()).expect("release");
    assert_eq!(watch_a.delivered.blocking_recv().expect("delivered"), 0);
    assert_eq!(watch_b.delivered.blocking_recv().expect("delivered"), 1);

    settle_all(&mut host);
    assert_eq!(
        host.state().settled_count,
        2,
        "both same-generation reads settled"
    );
    assert_eq!(host.state().stale_ignored, 0);
    let settled = host.state().settled.as_ref().expect("settled");
    assert_eq!(settled.generation, 0);
    let bytes = match (&settled.status, settled.asset) {
        (ReadStatus::Succeeded { bytes }, Some(id)) => {
            (bytes, host.asset(id).expect("blob retained").to_vec())
        }
        other => panic!("expected success, got {other:?}"),
    };
    assert!(
        bytes.1 == b"first-bytes".as_slice() || bytes.1 == b"second-bytes".as_slice(),
        "settled blob is one of the two reads"
    );
    assert_eq!(*bytes.0 as usize, bytes.1.len());
    assert_eq!(host.shutdown(), ShutdownReport { joined: 0 });
}

#[test]
fn gameplay_owner_advances_while_io_is_pending() {
    let asset = TempAsset::new("pending", b"pending-bytes");
    let (gate, mut watch) = gate_pair();
    let mut host = AssetHost::with_gates(vec![gate]);

    host.request_load(asset.path.clone());
    assert_eq!(watch.started.blocking_recv().expect("worker started"), 0);

    for expected in 1..=3 {
        let tick = host.tick();
        assert_eq!(tick, expected);
        assert!(host.state().settled.is_none(), "still pending");
        assert_eq!(host.state().settled_count, 0);
        assert_eq!(
            host.state().pending,
            vec![InFlight {
                request_id: 0,
                generation: 0,
                retired: false
            }]
        );
    }

    watch.proceed.blocking_send(()).expect("release worker");
    assert_eq!(watch.delivered.blocking_recv().expect("delivered"), 0);
    settle_all(&mut host);
    assert_eq!(
        host.state().settled.as_ref().expect("settled").status,
        ReadStatus::Succeeded { bytes: 13 }
    );
    assert_eq!(host.shutdown(), ShutdownReport { joined: 0 });
}

#[test]
fn scope_replacement_retires_pending_until_completion_arrives() {
    let asset = TempAsset::new("stale", b"stale-bytes");
    let (gate, mut watch) = gate_pair();
    let mut host = AssetHost::with_gates(vec![gate]);

    host.request_load(asset.path.clone());
    assert_eq!(watch.started.blocking_recv().expect("worker started"), 0);

    host.end_scope();
    assert_eq!(host.state().generation, 1);
    assert!(host.state().settled.is_none(), "scope results cleared");
    assert_eq!(host.state().settled_count, 0);
    assert_eq!(
        host.state().pending,
        vec![InFlight {
            request_id: 0,
            generation: 0,
            retired: true
        }],
        "capacity held until the late completion arrives"
    );

    watch.proceed.blocking_send(()).expect("release worker");
    assert_eq!(watch.delivered.blocking_recv().expect("delivered"), 0);
    settle_all(&mut host);

    assert_eq!(host.state().stale_ignored, 1, "completion counted stale");
    assert!(host.state().settled.is_none(), "replaced scope not mutated");
    assert!(
        host.state().pending.is_empty(),
        "capacity released on arrival"
    );

    // Capacity is free again, in the new generation.
    let later = TempAsset::new("stale-next", b"next-scope");
    assert_eq!(
        host.request_load(later.path.clone()),
        asset_loop::Admission::Accepted { request_id: 1 }
    );
    // This worker runs ungated; shutdown aborts its wrapper (the OS read may
    // already have started and cannot be cancelled) and joins it.
    assert_eq!(host.shutdown(), ShutdownReport { joined: 1 });
    assert!(
        host.state().pending.is_empty(),
        "terminal teardown releases"
    );
    assert!(
        host.state().settled.is_none() && host.state().settled_count == 0,
        "aborted read settles nothing"
    );
}

#[test]
fn duplicate_and_unknown_completions_never_free_capacity_or_overwrite() {
    let asset = TempAsset::new("dup", b"dup-bytes");
    let (gate, mut watch) = gate_pair();
    let mut host = AssetHost::with_gates(vec![gate]);

    host.request_load(asset.path.clone());
    assert_eq!(watch.started.blocking_recv().expect("worker started"), 0);
    watch.proceed.blocking_send(()).expect("release worker");
    assert_eq!(watch.delivered.blocking_recv().expect("delivered"), 0);
    settle_all(&mut host);

    let settled_before = host.state().settled.clone();

    // Duplicate of the settled completion: pending is empty, so it is unknown.
    host.deliver(AssetEvent::Loaded {
        request_id: 0,
        generation: 0,
        result: Ok(12),
    });
    // Unknown request.
    host.deliver(AssetEvent::Loaded {
        request_id: 7,
        generation: 0,
        result: Ok(7),
    });

    assert_eq!(host.state().stale_ignored, 2);
    assert_eq!(host.state().settled, settled_before);
    assert_eq!(host.state().settled_count, 1, "duplicates do not settle");
    assert_eq!(host.asset(AssetId(0)), Some(b"dup-bytes".as_slice()));
    assert_eq!(host.shutdown(), ShutdownReport { joined: 0 });
}

#[test]
fn saturated_admission_and_sequential_loads_keep_tasks_and_blobs_bounded() {
    let first = TempAsset::new("sat-1", b"first");
    let second = TempAsset::new("sat-2", b"second");
    let third = TempAsset::new("sat-3", b"third");
    let (gate_a, mut watch_a) = gate_pair();
    let (gate_b, mut watch_b) = gate_pair();
    let mut pairs = Vec::new();
    let mut watches = Vec::new();
    for _ in 2..6u64 {
        let (gate, watch) = gate_pair();
        pairs.push(gate);
        watches.push(watch);
    }
    let mut all_gates = vec![gate_a, gate_b];
    all_gates.extend(pairs);
    let mut host = AssetHost::with_gates(all_gates);

    assert_eq!(
        host.request_load(first.path.clone()),
        asset_loop::Admission::Accepted { request_id: 0 }
    );
    assert_eq!(
        host.request_load(second.path.clone()),
        asset_loop::Admission::Accepted { request_id: 1 }
    );
    assert_eq!(
        host.request_load(third.path.clone()),
        asset_loop::Admission::Rejected {
            in_flight: MAX_IN_FLIGHT
        }
    );
    assert_eq!(host.state().rejected, 1);

    // Both admitted workers took their requests; release them deterministically.
    assert_eq!(watch_a.started.blocking_recv().expect("started"), 0);
    assert_eq!(watch_b.started.blocking_recv().expect("started"), 1);
    watch_a.proceed.blocking_send(()).expect("release");
    watch_b.proceed.blocking_send(()).expect("release");
    assert!(watch_a.delivered.blocking_recv().is_some());
    assert!(watch_b.delivered.blocking_recv().is_some());

    settle_all(&mut host);
    assert_eq!(host.state().settled_count, 2);
    assert_eq!(host.unreaped_tasks(), 0, "finished handles reaped per tick");

    // Sequential loads reuse the window; task slots and blob ring stay bounded.
    let sequential: Vec<TempAsset> = (0..4)
        .map(|_| TempAsset::new("seq", b"seq-bytes"))
        .collect();
    for (index, asset) in sequential.into_iter().enumerate() {
        let request_id = 2 + index as u64;
        assert_eq!(
            host.request_load(asset.path.clone()),
            asset_loop::Admission::Accepted { request_id }
        );
        assert!(
            host.unreaped_tasks() <= MAX_IN_FLIGHT,
            "task bookkeeping bounded: {}",
            host.unreaped_tasks()
        );
        let watch = &mut watches[index];
        assert_eq!(watch.started.blocking_recv().expect("started"), request_id);
        watch.proceed.blocking_send(()).expect("release");
        assert_eq!(
            watch.delivered.blocking_recv().expect("delivered"),
            request_id
        );
        settle_all(&mut host);
        assert_eq!(host.unreaped_tasks(), 0, "reaped after each settled load");
        assert!(
            host.state().settled_count == 2 + index as u64 + 1,
            "every sequential load settles"
        );
        drop(asset);
    }
    // Blob ring holds at most BLOB_CAPACITY entries at any time.
    assert!(host.bounded_blob_entries() <= BLOB_CAPACITY);
    assert_eq!(host.state().settled_count, 6);
    assert_eq!(host.shutdown(), ShutdownReport { joined: 0 }, "all reaped");
}

#[test]
fn shutdown_is_terminal_and_aborts_parked_workers() {
    let asset = TempAsset::new("shutdown", b"shutdown-bytes");
    let (gate, mut watch) = gate_pair();
    let mut host = AssetHost::with_gates(vec![gate]);

    host.request_load(asset.path.clone());
    assert_eq!(watch.started.blocking_recv().expect("worker started"), 0);
    // The worker parks before its read, so no OS read was started; the abort
    // therefore cancels only the parked wrapper. See the module docs for the
    // honest cancellation limits once a read has begun.

    let report = host.shutdown();
    assert_eq!(report, ShutdownReport { joined: 1 });
    assert_eq!(host.state().generation, 1, "scope invalidated");
    assert!(
        host.state().pending.is_empty(),
        "terminal teardown releases"
    );
    assert_eq!(host.unreaped_tasks(), 0, "joined handles accounted");

    // Terminal: further admission is rejected without dispatch.
    let after = TempAsset::new("after", b"after-bytes");
    assert!(matches!(
        host.request_load(after.path.clone()),
        asset_loop::Admission::Rejected { .. }
    ));
    assert_eq!(host.state().next_request_id, 1, "no request was admitted");
    assert_eq!(host.shutdown(), ShutdownReport { joined: 0 }, "idempotent");

    // The parked worker was aborted before its read; releasing it must fail
    // because its receiver is gone, and no completion can arrive afterward.
    assert!(watch.proceed.blocking_send(()).is_err(), "receiver dropped");
    let tick = host.tick();
    assert_eq!(tick, 1, "gameplay tick still advances after shutdown");
    assert!(host.state().settled.is_none());
}

#[test]
fn emitted_tracing_fields_carry_request_and_generation() {
    // Worker events run on runtime threads, so the subscriber must be global.
    static INSTALL: std::sync::Once = std::sync::Once::new();
    let collector: Collector = Default::default();
    INSTALL.call_once(|| {
        let _ = tracing::subscriber::set_global_default(ShareEvents(collector.events.clone()));
    });

    let asset = TempAsset::new("tracing", b"tracing-bytes");
    let (gate, mut watch) = gate_pair();
    let mut host = AssetHost::with_gates(vec![gate]);
    host.request_load(asset.path.clone());
    assert_eq!(watch.started.blocking_recv().expect("started"), 0);
    watch.proceed.blocking_send(()).expect("release");
    assert!(watch.delivered.blocking_recv().is_some());
    settle_all(&mut host);
    host.shutdown();

    let events = collector.events.lock().unwrap();
    // Other tests in this binary share the global subscriber; select by path.
    let by_path = |suffix: &str| -> Option<Vec<(String, String)>> {
        events
            .iter()
            .find(|fields| {
                fields
                    .iter()
                    .any(|(k, v)| k == "path" && v.trim_matches('"').contains(suffix))
            })
            .cloned()
    };
    let admitted = by_path("-tracing-").expect("admission event");
    assert!(admitted.contains(&("request_id".into(), "0".into())));
    assert!(admitted.contains(&("generation".into(), "0".into())));
    // The settled event carries the same path, request and generation.
    let settled = {
        let fields = events
            .iter()
            .find(|fields| {
                fields
                    .iter()
                    .any(|(k, v)| k == "message" && v.contains("settled"))
                    && fields
                        .iter()
                        .any(|(k, v)| k == "path" && v.trim_matches('"').contains("-tracing-"))
            })
            .cloned();
        fields.expect("settled worker event")
    };
    assert!(settled.contains(&("request_id".into(), "0".into())));
    assert!(settled.contains(&("generation".into(), "0".into())));
}

#[test]
fn slot_bound_rejection_is_counted_by_a_reducer_event() {
    let first = TempAsset::new("slot-1", b"slot-one");
    let second = TempAsset::new("slot-2", b"slot-two");
    let later = TempAsset::new("slot-3", b"slot-three");
    let (gate_a, mut watch_a) = gate_pair();
    let (gate_b, mut watch_b) = gate_pair();
    let mut host = AssetHost::with_gates(vec![gate_a, gate_b]);

    assert_eq!(
        host.request_load(first.path.clone()),
        asset_loop::Admission::Accepted { request_id: 0 }
    );
    assert_eq!(
        host.request_load(second.path.clone()),
        asset_loop::Admission::Accepted { request_id: 1 }
    );
    // Both workers hold task slots; the third request is rejected by an
    // explicit reducer event, not a host-side state mutation.
    assert!(matches!(
        host.request_load(later.path.clone()),
        asset_loop::Admission::Rejected { .. }
    ));
    assert_eq!(host.state().rejected, 1, "counted inside the reducer");

    watch_a.proceed.blocking_send(()).expect("release");
    watch_b.proceed.blocking_send(()).expect("release");
    assert!(watch_a.delivered.blocking_recv().is_some());
    assert!(watch_b.delivered.blocking_recv().is_some());
    settle_all(&mut host);
    assert_eq!(host.state().settled_count, 2);
    assert_eq!(host.shutdown(), ShutdownReport { joined: 0 });
}

#[test]
fn completion_wait_times_out_and_terminal_cleanup_releases_entries() {
    let asset = TempAsset::new("timeout", b"timeout-bytes");
    let (gate, mut watch) = gate_pair();
    let mut host = AssetHost::with_gates(vec![gate]);

    host.request_load(asset.path.clone());
    assert_eq!(watch.started.blocking_recv().expect("started"), 0);
    // The worker parks before its read; the bounded completion wait expires.
    let waited = host.runtime_handle().block_on(async {
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            watch.delivered.recv(),
        )
        .await
    });
    assert!(waited.is_err(), "parked worker must hit the timeout");

    let report = host.shutdown();
    assert_eq!(report, ShutdownReport { joined: 1 });
    assert_eq!(host.state().generation, 1);
    assert!(host.state().pending.is_empty(), "terminal cleanup releases");
    assert!(host.state().settled.is_none() && host.state().settled_count == 0);
    // Terminal: admission stays rejected afterward.
    let after = TempAsset::new("timeout-after", b"after-bytes");
    assert!(matches!(
        host.request_load(after.path.clone()),
        asset_loop::Admission::Rejected { .. }
    ));
}

type SharedEvents = Arc<Mutex<Vec<Vec<(String, String)>>>>;

struct Collector {
    events: SharedEvents,
}

impl Default for Collector {
    fn default() -> Self {
        Collector {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

struct ShareEvents(SharedEvents);

impl tracing::Subscriber for ShareEvents {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::Id {
        tracing::Id::from_u64(1)
    }
    fn record(&self, _span: &tracing::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _from: &tracing::Id, _to: &tracing::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut fields: Vec<(String, String)> = Vec::new();
        event.record(
            &mut |field: &tracing::field::Field, value: &dyn std::fmt::Debug| {
                fields.push((field.name().to_string(), format!("{value:?}")));
            },
        );
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(fields);
    }
    fn enter(&self, _id: &tracing::Id) {}
    fn exit(&self, _id: &tracing::Id) {}
}
