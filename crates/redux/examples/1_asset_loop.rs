//! Executable proof: one real Tokio-backed asset-file read effect riding an existing
//! Redux [`redux::Slice`], with gameplay ticking that stays deterministic and never
//! awaits host I/O.
//!
//! # Ownership boundaries
//!
//! | kind | lives in | law |
//! | --- | --- | --- |
//! | reducer state | `AssetState` (plain data) | request IDs, generation, pending/status records; never byte blobs |
//! | asset blobs | `AssetHost::blobs` | host-owned ring holding exactly the latest settled blob, cleared on scope end |
//! | handles/tasks | `AssetHost::tasks` | reaped at every admission and tick; no parallel counter |
//! | host I/O runtime | `AssetHost::runtime` | one runtime per host scope; shutdown is terminal |
//! | gameplay ticks | `AssetEvent::Tick` | advances with zero I/O dependence |
//!
//! # Lifecycle (read/write sequence)
//!
//! ```text
//! frame owner (sync, never awaits)                 Tokio worker (tokio::fs::read)
//! ─────────────────────────────────────            ────────────────────────────
//! 1. reap finished handles                         4. await gate (test only)
//! 2. dispatch Load { path }; reducer:              5. tokio::fs::read(path).await
//!    pending.len() < MAX_IN_FLIGHT ? push          6. completions.send(Loaded).await (bounded)
//!    InFlight { request_id, generation,            7. gate.delivered.send (test only)
//!       retired: false } and emit Load effect
//! 3. tick(): drain <= DRAIN_PER_TICK completions;
//!    bytes are held beside each event; each Loaded
//!    is classified in ONE reducer transition (exact
//!    (request_id, generation) match against pending,
//!    entry removed to release capacity, status/IDs
//!    settled, StoreBlob emitted); the host stores the
//!    payload in its single-latest blob ring; then reap
//! end_scope(): generation += 1, every pending entry becomes retired, settled
//! records and host blobs clear; a late completion still removes its retired
//! entry (releasing capacity) but is counted stale and settles nothing.
//! shutdown(): terminal. end_scope, abort every handle, block_on join each
//! (this host is a synchronous Runtime owner; no async fn wraps the
//! block_on), then release the retired entries whose workers can never
//! deliver. request_load is rejected after shutdown.
//!
//! Cancellation honesty: aborting the async wrapper of a `tokio::fs::read`
//! cannot cancel an OS read that spawn_blocking already started; joining the
//! wrappers proves the async tasks ended, and any unobserved read is bounded by
//! the admission window. Runtime drop may itself wait for blocking work. Reads
//! are therefore never aborted at end_scope: they run to completion and retire
//! their own pending entry, which is what releases capacity. No hard latency
//! or cancellation bound is claimed.
//! ```
//!
//! Every arrow crosses an ownership move: the effect is owned by the worker, the
//! completion is owned by the receiver; nothing borrowed crosses an await and no
//! tracing guard is ever entered manually.

#![allow(dead_code)]

use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use redux::Slice;
use redux::reduce_then_apply;
use tokio::sync::mpsc;

/// Bounded admission: at most this many read requests in flight per scope,
/// tracked by the reducer's `pending` list (active and retired entries). An
/// over-bound `Load` is rejected synchronously inside the frame owner tick and
/// can never block it. Capacity is released only by completion delivery, so a
/// scope change that aborts nothing cannot inflate outstanding read work.
pub const MAX_IN_FLIGHT: usize = 2;

/// Bounded completion delivery: workers await `send` into this channel when
/// full, so backpressure parks workers only; the frame owner still drains with
/// `try_recv` and never waits.
pub const BOUNDED_COMPLETIONS: usize = 4;

/// Bounded per-tick completion drain; leftovers wait for the next tick.
pub const DRAIN_PER_TICK: usize = 4;

/// Explicit fixed capacity of the host blob ring; the oldest settled blob is
/// evicted when a new one pushes past it. Concurrent requests are observed
/// through `settled_count`, never through unbounded result history. No cache
/// framework.
pub const BLOB_CAPACITY: usize = MAX_IN_FLIGHT;

/// Bounded wait for one completion in the demonstration driver.
pub const COMPLETION_TIMEOUT: Duration = Duration::from_secs(5);

/// Host-owned handle to one stored asset blob; equals its request ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetId(pub u64);

/// Reducer-visible outcome of one settled read. Blob bytes never enter reducer
/// state; only the byte count for successes and the OS message for failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadStatus {
    Succeeded { bytes: u64 },
    Failed { message: String },
}

/// One outstanding read request, keyed by `(request_id, generation)`. `retired`
/// marks entries of a superseded scope: they keep occupying admission capacity
/// until their completion arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InFlight {
    pub request_id: u64,
    pub generation: u64,
    pub retired: bool,
}

/// Reducer record of the most recent settled request. `asset` is a host-side
/// blob handle, present only for successes of the current generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettledRequest {
    pub request_id: u64,
    pub generation: u64,
    pub status: ReadStatus,
    pub asset: Option<AssetId>,
}

/// Reducer state: plain data only. No handles, no channels, no runtime, no blobs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AssetState {
    /// Scope epoch. Bumped only on scope exit; completions from an older epoch
    /// retire their pending entry and settle nothing.
    pub generation: u64,
    /// Request IDs are unique within the host lifetime.
    pub next_request_id: u64,
    /// Outstanding read work; the admission bound across scope changes.
    pub pending: Vec<InFlight>,
    /// Deterministic gameplay tick, advanced by `Tick` regardless of I/O.
    pub tick: u64,
    /// `Load` events rejected because the admission window was full.
    pub rejected: u64,
    /// Completions ignored: stale generation, unknown or duplicate request.
    pub stale_ignored: u64,
    /// The most recent settled request of the current scope.
    pub settled: Option<SettledRequest>,
    /// How many requests settled in the current scope.
    pub settled_count: u64,
}

/// Events reduced by [`AssetLoader`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetEvent {
    /// Deterministic gameplay step; independent of any host I/O.
    Tick,
    /// Request an asset read; the reducer stamps it and emits the effect.
    Load { path: String },
    /// Owned completion from a host worker, status only: bytes stay with the
    /// host, which stores them after the reducer's `Accepted` output.
    Loaded {
        request_id: u64,
        generation: u64,
        result: Result<u64, String>,
    },
    /// Admission rejection outside the pending window (retained task slots).
    RejectLoad,
    /// Host scope exit. Non-terminal: entries stay retired until their
    /// completions arrive. Terminal (shutdown): entries can never complete and
    /// are released here.
    ScopeEnded { terminal: bool },
}

/// Inert effect descriptors: owned values handed to the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetEffect {
    /// Admitted request; the host spawns one worker per effect.
    Load {
        request_id: u64,
        generation: u64,
        path: String,
    },
}

/// What one reducer transition decided; the host acts on it without touching
/// state itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReduceOutcome {
    /// Loaded completion accepted and settled.
    Accepted,
    /// Loaded completion stale: unknown, duplicate or superseded generation.
    Stale,
    /// Load request admitted; the matching effect carries the request.
    Admitted,
    /// Load request rejected by the admission bound.
    Rejected,
    /// Non-asset event (Tick or scope change).
    Advanced,
}

/// The asset slice. Output states what the transition decided.
pub struct AssetLoader;

impl Slice for AssetLoader {
    type Context<'a> = ();
    type State = AssetState;
    type Event = AssetEvent;
    type Output = ReduceOutcome;
    type Effect = AssetEffect;

    fn reduce(
        st: &mut Self::State,
        ev: Self::Event,
        _cx: Self::Context<'_>,
        fx: &mut impl FnMut(Self::Effect),
    ) -> Self::Output {
        match ev {
            AssetEvent::Tick => {
                st.tick += 1;
                return ReduceOutcome::Advanced;
            }
            AssetEvent::Load { path } => {
                if st.pending.len() >= MAX_IN_FLIGHT {
                    st.rejected += 1;
                    return ReduceOutcome::Rejected;
                } else {
                    let request_id = st.next_request_id;
                    st.next_request_id += 1;
                    st.pending.push(InFlight {
                        request_id,
                        generation: st.generation,
                        retired: false,
                    });
                    fx(AssetEffect::Load {
                        request_id,
                        generation: st.generation,
                        path,
                    });
                    return ReduceOutcome::Admitted;
                }
            }
            AssetEvent::RejectLoad => {
                st.rejected += 1;
                return ReduceOutcome::Rejected;
            }
            AssetEvent::Loaded {
                request_id,
                generation,
                result,
            } => {
                let Some(position) = st.pending.iter().position(|entry| {
                    entry.request_id == request_id && entry.generation == generation
                }) else {
                    // Duplicate or unknown completion: never frees capacity
                    // and never overwrites settled state.
                    st.stale_ignored += 1;
                    return ReduceOutcome::Stale;
                };
                st.pending.remove(position);
                if generation != st.generation {
                    // Superseded scope: capacity released, nothing settled.
                    st.stale_ignored += 1;
                    return ReduceOutcome::Stale;
                }
                let (status, asset) = match result {
                    Ok(bytes) => (ReadStatus::Succeeded { bytes }, Some(AssetId(request_id))),
                    Err(message) => (ReadStatus::Failed { message }, None),
                };
                st.settled_count += 1;
                st.settled = Some(SettledRequest {
                    request_id,
                    generation,
                    status,
                    asset,
                });
                return ReduceOutcome::Accepted;
            }
            AssetEvent::ScopeEnded { terminal } => {
                st.generation += 1;
                for entry in &mut st.pending {
                    entry.retired = true;
                }
                st.settled = None;
                st.settled_count = 0;
                if terminal {
                    // No completion can follow a terminal teardown; the
                    // retired entries are released here, inside the reducer.
                    st.pending.clear();
                }
                return ReduceOutcome::Advanced;
            }
        }
    }
}

/// Deterministic handshake for one spawned read. The worker reports the request
/// it took, parks until the caller releases it, and reports the delivery of its
/// owned completion. Hosts without gates skip every handshake.
pub struct Gate {
    pub started: mpsc::Sender<u64>,
    pub proceed: mpsc::Receiver<()>,
    pub delivered: mpsc::Sender<u64>,
}

/// Synchronous admission outcome for one `request_load` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Accepted { request_id: u64 },
    Rejected { in_flight: usize },
}

/// Result of [`AssetHost::shutdown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownReport {
    pub joined: usize,
}

/// Owned request moved into the spawned worker.
struct ReadRequest {
    request_id: u64,
    generation: u64,
    path: String,
}

/// Owned completion moved from worker to host through the bounded channel.
/// Bytes ride here only; the host reduces them to a status before dispatch.
struct Completion {
    request_id: u64,
    generation: u64,
    result: Result<Vec<u8>, String>,
}

/// Host scope: owns the runtime, the bounded completion channel, every spawned
/// task and the asset blob ring. None of these are reducer state; `end_scope`
/// or `shutdown` is the scope exit, and `shutdown` is terminal.
pub struct AssetHost {
    state: AssetState,
    runtime: tokio::runtime::Runtime,
    completions_tx: Arc<mpsc::Sender<Completion>>,
    completions_rx: mpsc::Receiver<Completion>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    gates: Vec<Gate>,
    blobs: HashMap<u64, Vec<u8>>,
    blob_order: Vec<u64>,
    terminated: bool,
}

impl AssetHost {
    /// One runtime and one task set per host scope.
    pub fn new() -> Self {
        Self::build(Vec::new())
    }

    /// Test constructor: one handshake per admitted read, in spawn order.
    pub fn with_gates(gates: Vec<Gate>) -> Self {
        Self::build(gates)
    }

    fn build(gates: Vec<Gate>) -> Self {
        let (completions_tx, completions_rx) = mpsc::channel::<Completion>(BOUNDED_COMPLETIONS);
        Self {
            state: AssetState::default(),
            runtime: tokio::runtime::Runtime::new().expect("tokio runtime"),
            completions_tx: Arc::new(completions_tx),
            completions_rx,
            tasks: Vec::new(),
            gates,
            blobs: HashMap::new(),
            blob_order: Vec::new(),
            terminated: false,
        }
    }

    /// The reducer state; plain data only.
    pub fn state(&self) -> &AssetState {
        &self.state
    }

    /// How many blob entries the ring currently retains; at most `BLOB_CAPACITY`.
    pub fn bounded_blob_entries(&self) -> usize {
        self.blob_order.len()
    }

    /// Host-owned bytes for a settled asset of the current scope.
    pub fn asset(&self, id: AssetId) -> Option<&[u8]> {
        self.blobs.get(&id.0).map(|bytes| bytes.as_slice())
    }

    /// Spawned tasks not yet reaped by a tick or joined by shutdown. Derived
    /// from the owned handle vector; there is no parallel counter.
    pub fn unreaped_tasks(&self) -> usize {
        self.tasks.len()
    }

    /// Remove handles whose worker already finished. Called before every
    /// admission and at every tick, so the retained set stays bounded by the
    /// admission window plus at most one in-flight completion drain.
    fn reap(&mut self) {
        self.tasks.retain(|task| !task.is_finished());
    }

    /// One sync frame: drain at most `DRAIN_PER_TICK` completions with
    /// `try_recv`, then reap finished handles, then dispatch one gameplay
    /// `Tick`. Never awaits host I/O. Returns the tick count.
    pub fn tick(&mut self) -> u64 {
        for _ in 0..DRAIN_PER_TICK {
            let Some(completion) = self.completions_rx.try_recv().ok() else {
                break;
            };
            let _ = self.deliver_completion(completion);
        }
        self.reap();
        let _ = self.dispatch_event(AssetEvent::Tick);
        self.state.tick
    }

    /// Ingest one owned completion: bytes stay with the host, the reducer
    /// classifies the status-only event by exact `(request_id, generation)` in
    /// one transition, and an `Accepted` output is what moves the held bytes
    /// into the bounded ring. Unknown or duplicate completions free no
    /// capacity, settle nothing and drop their bytes. Never blocks the frame
    /// owner.
    fn deliver_completion(&mut self, completion: Completion) -> ReduceOutcome {
        let Completion {
            request_id,
            generation,
            result,
        } = completion;
        let (payload, result) = match result {
            Ok(bytes) => {
                let bytes_len = bytes.len() as u64;
                (Some((request_id, bytes)), Ok(bytes_len))
            }
            Err(message) => (None, Err(message)),
        };
        let outcome = self.dispatch_event(AssetEvent::Loaded {
            request_id,
            generation,
            result,
        });
        if outcome == ReduceOutcome::Accepted {
            if let Some((id, bytes)) = payload {
                self.store_blob(id, bytes);
            }
        }
        outcome
    }

    /// Ingest one status-only event (test seam for duplicate/unknown
    /// completions); no bytes are involved, so nothing is stored.
    pub fn deliver(&mut self, event: AssetEvent) -> ReduceOutcome {
        self.dispatch_event(event)
    }

    /// Store one accepted blob, evicting the previous entry past
    /// `BLOB_CAPACITY` so exactly the latest settled blob is retained.
    fn store_blob(&mut self, request_id: u64, bytes: Vec<u8>) {
        if self.blobs.contains_key(&request_id) {
            self.blobs.insert(request_id, bytes);
            return;
        }
        self.blobs.insert(request_id, bytes);
        self.blob_order.push(request_id);
        if self.blob_order.len() > BLOB_CAPACITY {
            let evicted = self.blob_order.remove(0);
            self.blobs.remove(&evicted);
        }
    }

    /// Admit one read request. Synchronous: finished handles are reaped first,
    /// then the reducer either admits (effect emitted, task spawned) or rejects
    /// in-state. Retained task slots are part of the bound: if they already
    /// fill the window, the request is rejected without dispatch. Never blocks
    /// the frame owner. Rejected after `shutdown`, which is terminal.
    pub fn request_load(&mut self, path: impl Into<String>) -> Admission {
        if self.terminated {
            return Admission::Rejected {
                in_flight: self.tasks.len(),
            };
        }
        self.reap();
        if self.tasks.len() >= MAX_IN_FLIGHT {
            // Retained task slots are part of the admission bound (a worker
            // parked on its delivery handshake holds a slot without a pending
            // entry). The rejection is counted by an explicit reducer event;
            // the host never mutates reducer state itself.
            let _ = self.dispatch_event(AssetEvent::RejectLoad);
            return Admission::Rejected {
                in_flight: self.tasks.len(),
            };
        }
        let mut scratch = Vec::new();
        let mut emitted = Vec::new();
        let outcome = reduce_then_apply::<AssetLoader, _>(
            &mut self.state,
            AssetEvent::Load { path: path.into() },
            (),
            &mut scratch,
            |_, effect| emitted.push(effect),
        );
        match (outcome, emitted.pop()) {
            (
                ReduceOutcome::Admitted,
                Some(AssetEffect::Load {
                    request_id,
                    generation,
                    path,
                }),
            ) => {
                self.spawn_read(ReadRequest {
                    request_id,
                    generation,
                    path,
                });
                Admission::Accepted { request_id }
            }
            _ => Admission::Rejected {
                in_flight: self.state.pending.len(),
            },
        }
    }

    /// Scope exit: bump the generation, retire every pending entry, clear the
    /// scope's settled records and host blobs. Reads keep running and retire
    /// their own entries on arrival; nothing is aborted here.
    pub fn end_scope(&mut self) {
        let _ = self.dispatch_event(AssetEvent::ScopeEnded { terminal: false });
        self.blobs.clear();
        self.blob_order.clear();
    }

    /// Terminal teardown: `end_scope`, abort every handle, then join them all.
    /// This host is a synchronous runtime owner, so the joins are awaited
    /// through one `block_on` outside any async context. Aborting a wrapper
    /// around `tokio::fs::read` cannot cancel an OS read that already started;
    /// the join proves the async tasks ended, and any unobserved read was
    /// bounded by the admission window. Runtime drop may itself wait for
    /// blocking work. After this call `request_load` is rejected, so reads
    /// cannot accumulate across shutdown/reuse cycles; retired pending entries
    /// whose workers can never deliver are released here.
    pub fn shutdown(&mut self) -> ShutdownReport {
        if self.terminated {
            return ShutdownReport { joined: 0 };
        }
        self.terminated = true;
        // Terminal scope exit: the reducer releases the retired entries that
        // can never complete.
        let _ = self.dispatch_event(AssetEvent::ScopeEnded { terminal: true });
        self.blobs.clear();
        self.blob_order.clear();
        let handles = std::mem::take(&mut self.tasks);
        for handle in &handles {
            handle.abort();
        }
        let joined = self.runtime.block_on(async {
            let mut joined = 0;
            for handle in handles {
                let _ = handle.await;
                joined += 1;
            }
            joined
        });
        ShutdownReport { joined }
    }

    /// Dispatch one event through the asset slice.
    fn dispatch_event(&mut self, event: AssetEvent) -> ReduceOutcome {
        let mut scratch = Vec::new();
        reduce_then_apply::<AssetLoader, _>(&mut self.state, event, (), &mut scratch, |_, _| {})
    }

    /// Handle onto the host runtime, for the synchronous demonstration driver
    /// to wait on completions with a bounded timeout.
    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
    }

    /// Spawn one owned read worker. The request is moved in; the completion is
    /// moved out through the bounded channel.
    fn spawn_read(&mut self, request: ReadRequest) {
        let completions = self.completions_tx.clone();
        let gate = if self.gates.is_empty() {
            None
        } else {
            Some(self.gates.remove(0))
        };
        let task = self.runtime.spawn(read_asset(request, completions, gate));
        self.tasks.push(task);
    }
}

/// One asset read: optional test handshake, the real file read, one owned
/// completion through the bounded channel. Instrumented per the tracing
/// standard with `skip_all` and explicitly selected scalar fields; no span
/// guard is entered manually anywhere, so nothing is held across an await.
#[tracing::instrument(skip_all, fields(request_id = request.request_id, generation = request.generation))]
async fn read_asset(
    request: ReadRequest,
    completions: Arc<mpsc::Sender<Completion>>,
    gate: Option<Gate>,
) {
    let ReadRequest {
        request_id,
        generation,
        path,
    } = request;
    let mut gate = gate;
    if let Some(handshake) = gate.as_mut() {
        if handshake.started.send(request_id).await.is_err() {
            return;
        }
        if handshake.proceed.recv().await.is_none() {
            return;
        }
    }
    // Real I/O through tokio::fs. Once the underlying blocking read has started,
    // aborting this wrapper cannot cancel it; the completion (or shutdown join)
    // is what bounds and accounts the work.
    let outcome = tokio::fs::read(&path)
        .await
        .map_err(|error| error.to_string());
    tracing::debug!(request_id, generation, path = %path, "asset read settled");
    if completions
        .send(Completion {
            request_id,
            generation,
            result: outcome,
        })
        .await
        .is_ok()
    {
        if let Some(handshake) = gate {
            let _ = handshake.delivered.send(request_id).await;
        }
    }
}

/// Runnable demonstration: write a unique temporary asset, request its read,
/// advance the gameplay tick while the read is in flight, settle it, then shut
/// the scope down. Completion waiting happens here, outside the frame-step
/// method, with a bounded iteration budget; any unexpected result exits
/// nonzero.
fn main() -> ExitCode {
    let scope = match tempfile::tempdir() {
        Ok(scope) => scope,
        Err(error) => {
            eprintln!("asset loop proof: tempdir failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let asset = scope.path().join("asset.bin");
    if let Err(error) = std::fs::write(&asset, b"proof-bytes") {
        eprintln!("asset loop proof: temp asset write failed: {error}");
        return ExitCode::FAILURE;
    }

    // Deterministic completion wait outside the frame-step method: the gate
    // handshakes plus one tokio::time::timeout bound the wait; exactly one
    // settling tick follows a delivered completion.
    let (started_tx, mut started_rx) = tokio::sync::mpsc::channel(1);
    let (proceed_tx, proceed_rx) = tokio::sync::mpsc::channel(1);
    let (delivered_tx, mut delivered_rx) = tokio::sync::mpsc::channel(1);
    let mut host = AssetHost::with_gates(vec![Gate {
        started: started_tx,
        proceed: proceed_rx,
        delivered: delivered_tx,
    }]);
    match host.request_load(asset.to_string_lossy().into_owned()) {
        Admission::Accepted { request_id } => {
            println!("admitted request {request_id}");
        }
        Admission::Rejected { in_flight } => {
            eprintln!("asset loop proof: rejected, {in_flight} in flight");
            return ExitCode::FAILURE;
        }
    }

    let handshakes = host.runtime_handle().block_on(async {
        tokio::time::timeout(COMPLETION_TIMEOUT, async {
            let started = started_rx.recv().await?;
            println!("worker {started} took the read");
            proceed_tx.send(()).await.ok()?;
            delivered_rx.recv().await
        })
        .await
    });
    let Some(delivered) = handshakes.ok().flatten() else {
        eprintln!("asset loop proof: completion wait failed or timed out");
        return ExitCode::FAILURE;
    };
    println!("worker {delivered} delivered its completion");

    let tick = host.tick();
    let settled = host.state().settled.clone();
    if let Some(record) = &settled {
        println!("tick {tick}: settled {record:?}");
    }

    let blob_ok = settled
        .as_ref()
        .and_then(|record| record.asset)
        .is_some_and(|id| host.asset(id) == Some(b"proof-bytes".as_slice()));
    let report = host.shutdown();
    println!("shutdown joined {} tasks", report.joined);
    match (settled, blob_ok) {
        (
            Some(SettledRequest {
                status: ReadStatus::Succeeded { bytes },
                ..
            }),
            true,
        ) => {
            println!("asset loop proof: ok ({bytes} bytes)");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("asset loop proof: unexpected {other:?}");
            ExitCode::FAILURE
        }
    }
}
