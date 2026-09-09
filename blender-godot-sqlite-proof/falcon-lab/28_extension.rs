#![cfg(feature = "gdext")]
// godot 0.4.5's #[class(init)] emits `base: base` in generated initialization.
#![allow(clippy::redundant_field_names)]
use godot::prelude::*;
use smash::fighters::falcon;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::JoinHandle;

use crate::fixture;
use fixture::sql_viewer::geometry;
#[path = "contracts/2_godot_auto.rs"]
#[allow(dead_code)]
mod payload;
use payload::{GodotFramePayload, GodotMeshReceipt};
use fixture::sql_viewer::boundary::contracts::{
    Acknowledgment, AckResult, BoundaryError, FrameAcknowledger, GenerationId, RowPublisher,
    pack_rows as pack, FrameStatus, ExternalStatus, ScheduledFrameStatus, ControlInput,
    CONTROL_TICKS,
};

struct Packet {
    rows: Vec<f64>,
    lines: Vec<geometry::Line>,
    status: FrameStatus,
}
impl FrameAcknowledger for Packet {
    fn acknowledge(&mut self, receipt: Acknowledgment) -> AckResult {
        if receipt.id.epoch != 0
            || receipt.id.generation != self.status.renderer_generation()
        {
            return Err(BoundaryError::StaleGeneration);
        }
        if receipt.row_digest != digest(&self.rows)
            || receipt.mesh_vertices as usize != self.lines.len() * 2
        {
            return Err(BoundaryError::InvalidPayload);
        }
        Ok(receipt.id)
    }
}
struct External {
    path: std::path::PathBuf,
    audit: std::path::PathBuf,
    boundary: fixture::sql_viewer::boundary::Boundary,
    source_generation: u64,
}
struct Bridge {
    requests: Option<SyncSender<u8>>,
    responses: Receiver<Packet>,
    worker: Option<JoinHandle<Result<(), String>>>,
}
struct Scheduled {
    shared: crate::schedule::State,
    worker: Option<JoinHandle<Result<Vec<crate::schedule::Status>, String>>>,
}
impl Drop for Scheduled {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn digest(values: &[f64]) -> u64 {
        values
            .iter()
            .flat_map(|v| v.to_bits().to_le_bytes())
            .fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b))
                .wrapping_mul(0x100000001b3))
}

struct FalconExtension;
#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn acknowledgement_checks_identity_digest_and_vertex_count() {
        let mut packet = Packet {
            rows: vec![0.0, 1.0, 2.0],
            lines: vec![],
            status: FrameStatus::External(ExternalStatus {
                source_pid: 1, source_generation: 1, renderer_generation: 7,
                published_tick: 0, source_elapsed_us: 0, skipped_generations: 0,
                consumer_pid: 2, ipc_sql_exact: true,
            }),
        };
        let receipt = Acknowledgment {
            id: GenerationId { epoch: 0, generation: 7 },
            row_digest: digest(&packet.rows), mesh_vertices: 0,
        };
        for (candidate, expected) in [
            (receipt, Ok(receipt.id)),
            (Acknowledgment { id: GenerationId { epoch: 1, generation: 7 }, ..receipt }, Err(BoundaryError::StaleGeneration)),
            (Acknowledgment { id: GenerationId { epoch: 0, generation: 6 }, ..receipt }, Err(BoundaryError::StaleGeneration)),
            (Acknowledgment { row_digest: receipt.row_digest ^ 1, ..receipt }, Err(BoundaryError::InvalidPayload)),
            (Acknowledgment { mesh_vertices: 1, ..receipt }, Err(BoundaryError::InvalidPayload)),
        ] {
            assert_eq!(FrameAcknowledger::acknowledge(&mut packet, candidate), expected);
            assert_eq!(packet.rows, vec![0.0, 1.0, 2.0]);
            assert_eq!(packet.status.renderer_generation(), 7);
        }
    }
}

#[gdextension]
unsafe impl ExtensionLibrary for FalconExtension {}

#[derive(GodotClass)]
#[class(base=RefCounted, init)]
struct FalconSql {
    base: Base<RefCounted>,
    bridge: Option<Bridge>,
    pending: Option<Packet>,
    acknowledgements: Vec<serde_json::Value>,
    reference: Vec<serde_json::Value>,
    incremental: bool,
    scheduled: Option<Scheduled>,
    faults: bool,
    external: Option<External>,
    controlled: Option<crate::control::Controlled>,
}

#[godot_api]
impl FalconSql {
    #[func]
    fn start_controlled(&mut self, record: bool) {
        fixture::baseline::telemetry::init();
        assert!(self.controlled.is_none() && self.bridge.is_none());
        self.controlled = Some(crate::control::Controlled::new(record).unwrap());
    }

    #[func]
    fn control_ticks(&self) -> i64 { i64::from(CONTROL_TICKS) }

    #[func]
    fn control_demo_input(&self, tick: i64) -> VarDictionary {
        crate::control::demo_input(i32::try_from(tick).unwrap()).to_dictionary()
    }

    #[func]
    fn advance_controlled(&mut self, input: VarDictionary) -> VarDictionary {
        assert!(self.pending.is_none());
        let input = ControlInput::from_dictionary(&input);
        let (rows, status) = self.controlled.as_mut().unwrap().step(input).unwrap();
        self.deliver(Packet { rows: pack(&rows), lines: geometry::wire(&rows),
            status: FrameStatus::Controlled(status) })
    }

    #[func]
    fn finish_controlled(&mut self, path: GString) -> bool {
        assert!(self.pending.is_none());
        assert_eq!(self.acknowledgements.len(), CONTROL_TICKS as usize);
        let proof = self.controlled.as_ref().unwrap().verify();
        std::fs::write(path.to_string(), serde_json::to_vec_pretty(&proof).unwrap()).unwrap();
        godot_print!("CONTROL_OK ticks={} hits={} damage={} replayed={} rows_and_mesh=exact",
            proof.ticks, proof.hits, proof.damage, proof.replayed);
        true
    }

    #[func]
    fn start_external(&mut self, path: GString, audit: GString) {
        fixture::baseline::telemetry::init();
        assert!(self.bridge.is_none() && self.scheduled.is_none() && self.external.is_none());
        self.external = Some(External {
            path: path.to_string().into(),
            audit: audit.to_string().into(),
            boundary: fixture::sql_viewer::boundary::Boundary::new().unwrap(),
            source_generation: 0,
        });
    }

    #[func]
    #[tracing::instrument(target = "falcon::godot", level = "trace", skip_all)]
    fn poll_external(&mut self) -> VarDictionary {
        assert!(self.pending.is_none());
        let external = self.external.as_mut().unwrap();
        if !external.path.exists() {
            return VarDictionary::new();
        }
        let latest = crate::live_rows::read(&external.path).unwrap();
        if latest.generation == external.source_generation {
            return VarDictionary::new();
        }
        assert!(latest.generation > external.source_generation);
        let previous = external.source_generation;
        let tick = latest.rows[0].tick;
        RowPublisher::publish(&mut external.boundary, &latest.rows).unwrap();
        let (generation, rows) =
            fixture::sql_viewer::boundary::read_frame(&external.boundary.db, tick).unwrap();
        assert_eq!(rows, latest.rows);
        external.source_generation = latest.generation;
        let status = FrameStatus::External(ExternalStatus {
            source_pid: latest.pid, source_generation: latest.generation,
            renderer_generation: generation, published_tick: tick,
            source_elapsed_us: latest.elapsed_us,
            skipped_generations: latest.generation - previous - 1,
            consumer_pid: std::process::id(), ipc_sql_exact: true,
        });
        self.deliver(Packet {
            rows: pack(&rows),
            lines: geometry::wire(&rows),
            status,
        })
    }
    #[func]
    fn proof_version(&self) -> GString {
        "falcon-sql-gdext-1".into()
    }

    #[func]
    fn start(&mut self) {
        fixture::baseline::telemetry::init();
        assert!(self.bridge.is_none());
        self.reference = serde_json::from_slice(
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/24_sql_boundary_trace.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let (requests, request_rx) = mpsc::sync_channel(1);
        let (response_tx, responses) = mpsc::sync_channel(1);
        let incremental = self.incremental;
        let dispatch = tracing::dispatcher::get_default(Clone::clone);
        let parent = tracing::Span::current();
        let worker = std::thread::spawn(move || {
            let _dispatch = tracing::dispatcher::set_default(&dispatch);
            let _parent = parent.enter();
            if incremental {
                return fixture::incremental_host(
                    || Ok(request_rx.recv()?),
                    |rows, status| {
                        response_tx.send(Packet {
                            rows: pack(rows),
                            lines: geometry::wire(rows),
                            status: FrameStatus::Fixture(status.clone()),
                        })?;
                        Ok(())
                    },
                )
                .map_err(|error| error.to_string());
            }
            fixture::host_fixture(|rows, status| {
                request_rx.recv()?;
                response_tx.send(Packet {
                    rows: pack(rows),
                    lines: geometry::wire(rows),
                    status: FrameStatus::Fixture(status.clone()),
                })?;
                Ok(())
            })
            .map_err(|error| error.to_string())
        });
        self.bridge = Some(Bridge {
            requests: Some(requests),
            responses,
            worker: Some(worker),
        });
    }

    #[func]
    fn start_incremental(&mut self) {
        self.incremental = true;
        self.start();
    }

    #[func]
    fn start_scheduled(&mut self) {
        fixture::baseline::telemetry::init();
        assert!(self.bridge.is_none() && self.scheduled.is_none());
        let shared = crate::schedule::State::default();
        let worker = crate::schedule::spawn(shared.clone(), self.faults);
        self.scheduled = Some(Scheduled {
            shared,
            worker: Some(worker),
        });
    }

    #[func]
    fn start_faults(&mut self) {
        self.faults = true;
        self.start_scheduled();
    }

    #[func]
    #[tracing::instrument(target = "falcon::godot", level = "trace", skip_all, fields(consume))]
    fn poll_scheduled(&mut self, consume: bool) -> VarDictionary {
        assert!(self.pending.is_none());
        let scheduled = self.scheduled.as_ref().unwrap();
        let state = scheduled.shared.lock().unwrap();
        let mut result = VarDictionary::new();
        let Some(current) = &state.current else {
            return result;
        };
        result.set(
            "current",
            current.to_dictionary(),
        );
        let published = state.published.as_ref().unwrap();
        let previous = self
            .acknowledgements
            .last()
            .and_then(|v| v["renderer_generation"].as_u64());
        let packet = if consume
            && (self.faults || !(92..106).contains(&current.simulation_tick))
            && previous != Some(published.generation)
        {
            let reader =
                fixture::sql_viewer::boundary::reader_for(state.ring.as_ref().unwrap()).unwrap();
            let (generation, rows) =
                fixture::sql_viewer::boundary::read_frame(&reader, published.published_tick)
                    .unwrap();
            assert_eq!(generation, published.generation);
            let status = FrameStatus::Scheduled(ScheduledFrameStatus {
                simulation_tick: published.simulation_tick,
                published_tick: published.published_tick,
                generation: published.generation,
                skipped_publications: published.skipped_publications,
                published: published.published,
                advances: published.advances,
                restored: published.restored.clone(),
                held_generation: published.held_generation,
                held_damage: published.held_damage,
                fresh_tick91_damage: published.fresh_tick91_damage,
                rows: published.rows,
                window_frames: published.window_frames,
                renderer_generation: generation,
                observed_simulation_tick: current.simulation_tick,
            });
            Some(Packet {
                rows: pack(&rows),
                lines: geometry::wire(&rows),
                status,
            })
        } else {
            None
        };
        drop(state);
        if let Some(packet) = packet {
            result.set("frame", self.deliver(packet));
        }
        result
    }

    #[func]
    fn finish_scheduled(&mut self) -> bool {
        assert!(self.pending.is_none());
        let mut scheduled = self.scheduled.take().unwrap();
        let audit = scheduled.worker.take().unwrap().join().unwrap().unwrap();
        let last = self.acknowledgements.last().unwrap();
        assert_eq!(last["published_tick"], 179);
        assert_eq!(
            last["skipped_publications"],
            if self.faults { 0 } else { 12 }
        );
        // The consumer skipped the injected pause, then read the latest SQL
        // publication observed under the metadata lock, without a frame queue.
        assert!(
            self.faults
                || self
                    .acknowledgements
                    .windows(2)
                    .any(|pair| pair[0]["published_tick"].as_i64().unwrap() < 92
                        && pair[1]["published_tick"].as_i64().unwrap() >= 106)
        );
        for entry in &self.acknowledgements {
            assert_eq!(entry["published_tick"], entry["observed_simulation_tick"]);
        }
        for (path, value) in [
            (
                if self.faults {
                    "56_fault_worker.json"
                } else {
                    "47_worker_schedule.json"
                },
                serde_json::to_value(audit).unwrap(),
            ),
            (
                if self.faults {
                    "57_fault_consumed.json"
                } else {
                    "48_schedule_consumed.json"
                },
                serde_json::to_value(&self.acknowledgements).unwrap(),
            ),
        ] {
            std::fs::write(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path),
                serde_json::to_vec_pretty(&value).unwrap(),
            )
            .unwrap();
        }
        if self.faults {
            godot_print!(
                "FAULT_RUNTIME_OK ticks=180 full_states=360 exact consumer_latest=verified"
            );
        } else {
            godot_print!(
                "SCHEDULE_OK ticks=180 full_states=360 exact skipped_publications=12 consumer_latest=verified cursor_isolation=verified"
            );
        }
        true
    }

    #[func]
    fn next_frame(&mut self) -> VarDictionary {
        self.advance(i64::from(falcon::fixture_input(
            self.acknowledgements.len() as i32,
        )))
    }

    #[func]
    fn advance(&mut self, input: i64) -> VarDictionary {
        assert!(self.pending.is_none(), "previous frame not acknowledged");
        let bridge = self.bridge.as_ref().unwrap();
        bridge
            .requests
            .as_ref()
            .unwrap()
            .send(u8::try_from(input).unwrap())
            .unwrap();
        let packet = bridge.responses.recv().expect("Rust fixture failed");
        self.deliver(packet)
    }

    #[tracing::instrument(target = "falcon::godot", level = "trace", skip_all, fields(rows = packet.rows.len() / 27, lines = packet.lines.len()))]
    fn deliver(&mut self, packet: Packet) -> VarDictionary {
        let vertices: PackedVector3Array = packet
            .lines
            .iter()
            .flat_map(|l| [l.a, l.b])
            .map(|p| Vector3::new(p[2], p[1], -p[0]))
            .collect();
        let colors: PackedColorArray = packet
            .lines
            .iter()
            .flat_map(|l| [l.color; 2])
            .map(|c| Color::from_rgba(c[0], c[1], c[2], c[3]))
            .collect();
        let result = GodotFramePayload {
            rows: PackedFloat64Array::from(packet.rows.as_slice()),
            vertices, colors, status: packet.status.clone(),
        }.to_dictionary();
        self.pending = Some(packet);
        result
    }

    #[func]
    #[tracing::instrument(target = "falcon::godot", level = "trace", skip_all)]
    fn acknowledge(
        &mut self,
        receipt: VarDictionary,
    ) -> bool {
        let GodotMeshReceipt { generation, rows, vertices } =
            GodotMeshReceipt::from_dictionary(&receipt);
        let packet = self.pending.as_mut().unwrap();
        assert_eq!(
            packet.rows.as_slice(),
            rows.as_slice(),
            "Godot row roundtrip differs"
        );
        assert_eq!(
            packet.status.renderer_generation(),
            u64::try_from(generation).unwrap()
        );
        assert_eq!(packet.lines.len() * 2, vertices.len());
        for (expected, actual) in packet
            .lines
            .iter()
            .flat_map(|l| [l.a, l.b])
            .zip(vertices.as_slice())
        {
            assert_eq!(
                Vector3::new(expected[2], expected[1], -expected[0]),
                *actual,
                "Godot mesh upload changed a vertex"
            );
        }
        FrameAcknowledger::acknowledge(packet, Acknowledgment {
            id: GenerationId { epoch: 0, generation: u64::try_from(generation).unwrap() },
            row_digest: digest(rows.as_slice()),
            mesh_vertices: u32::try_from(vertices.len()).unwrap(),
        }).unwrap();
        let packet = self.pending.take().unwrap();
        let mut status = serde_json::to_value(packet.status).unwrap();
        status["row_digest"] = format!("{:016x}", digest(&packet.rows)).into();
        status["frame_rows"] = (packet.rows.len() / 27).into();
        status["mesh_vertices"] = vertices.len().into();
        status["row_roundtrip_exact"] = true.into();
        status["mesh_roundtrip_exact"] = true.into();
        if let Some(external) = &self.external {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&external.audit)
                .unwrap();
            writeln!(file, "{}", status).unwrap();
        }
        if self.controlled.as_ref().is_none_or(|run| run.recorded.is_some()) {
            self.acknowledgements.push(status);
        }
        true
    }

    #[func]
    fn finish(&mut self) -> bool {
        assert!(self.pending.is_none());
        assert_eq!(self.acknowledgements.len(), 180);
        assert_eq!(self.reference.len(), self.acknowledgements.len());
        for (expected, actual) in self.reference.iter().zip(&self.acknowledgements) {
            for (key, value) in expected.as_object().unwrap() {
                assert_eq!(
                    &actual[key], value,
                    "Godot differs from the recorded wgpu trace: {key}"
                );
            }
        }
        let mut bridge = self.bridge.take().unwrap();
        bridge.requests.take();
        bridge
            .worker
            .take()
            .unwrap()
            .join()
            .expect("fixture panic")
            .expect("fixture error");
        std::fs::write(
            if self.incremental {
                concat!(env!("CARGO_MANIFEST_DIR"), "/38_incremental_consumed.json")
            } else {
                concat!(env!("CARGO_MANIFEST_DIR"), "/30_godot_consumed.json")
            },
            serde_json::to_vec_pretty(&self.acknowledgements).unwrap(),
        )
        .unwrap();
        godot_print!(
            "GDEXT_SQL_OK frames=180 row_roundtrip=exact mesh_roundtrip=exact held_cursor=verified"
        );
        true
    }
}
