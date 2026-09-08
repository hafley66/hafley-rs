#![cfg(feature = "gdext")]
// godot 0.4.5's #[class(init)] emits `base: base` in generated initialization.
#![allow(clippy::redundant_field_names)]
use godot::prelude::*;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::JoinHandle;

use crate::fixture;
use fixture::sql_viewer::{boundary::Row, geometry};

struct Packet {
    rows: Vec<f64>,
    lines: Vec<geometry::Line>,
    status: serde_json::Value,
}
struct Bridge {
    requests: Option<SyncSender<u8>>,
    responses: Receiver<Packet>,
    worker: Option<JoinHandle<Result<(), String>>>,
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn pack(rows: &[Row]) -> Vec<f64> {
    rows.iter()
        .flat_map(|r| {
            [r.tick as f64, r.kind as f64, r.entity as f64]
                .into_iter()
                .chain(r.values)
        })
        .collect()
}
fn digest(values: &[f64]) -> String {
    format!(
        "{:016x}",
        values
            .iter()
            .flat_map(|v| v.to_bits().to_le_bytes())
            .fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b))
                .wrapping_mul(0x100000001b3))
    )
}

struct FalconExtension;
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
}

#[godot_api]
impl FalconSql {
    #[func]
    fn proof_version(&self) -> GString {
        "falcon-sql-gdext-1".into()
    }

    #[func]
    fn start(&mut self) {
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
        let worker = std::thread::spawn(move || {
            if incremental {
                return fixture::incremental_host(
                    || Ok(request_rx.recv()?),
                    |rows, status| {
                        response_tx.send(Packet {
                            rows: pack(rows),
                            lines: geometry::wire(rows),
                            status: status.clone(),
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
                    status: status.clone(),
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
    fn next_frame(&mut self) -> VarDictionary {
        self.advance(i64::from(falcon_simulation::fixture_input(
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
        let mut result = VarDictionary::new();
        result.set("rows", PackedFloat64Array::from(packet.rows.as_slice()));
        result.set("vertices", vertices);
        result.set("colors", colors);
        result.set("status", GString::from(&packet.status.to_string()));
        self.pending = Some(packet);
        result
    }

    #[func]
    fn acknowledge(
        &mut self,
        generation: i64,
        rows: PackedFloat64Array,
        vertices: PackedVector3Array,
    ) -> bool {
        let packet = self.pending.take().unwrap();
        assert_eq!(
            packet.rows.as_slice(),
            rows.as_slice(),
            "Godot row roundtrip differs"
        );
        assert_eq!(
            packet.status["renderer_generation"].as_i64(),
            Some(generation)
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
        let mut status = packet.status;
        status["row_digest"] = digest(&packet.rows).into();
        status["frame_rows"] = (packet.rows.len() / 27).into();
        status["mesh_vertices"] = vertices.len().into();
        status["row_roundtrip_exact"] = true.into();
        status["mesh_roundtrip_exact"] = true.into();
        self.acknowledgements.push(status);
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
