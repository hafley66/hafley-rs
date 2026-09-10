//! Executable status export. Emits the observed Rust-owned facts that
//! `classification/8_status.mjs` joins against authored TypeSpec and retained
//! ingest records. Read-only: no files are written.
//!
//! Run: `cargo run --locked --offline -j2 --manifest-path smash/Cargo.toml \
//!   --example status_export`

use game_fighter::Phase;
use game_fighter::status::{air_transitions, ground_transitions};
use serde_json::{Value, json};
use smash::fighters::falcon::{catalog, movement};

/// Deterministic stick sweep; the phase->animation seam is interpolated only at
/// these points, so band changes appear as separate observed actions.
const AXIS_PROBES: [f32; 21] = [
    -1.0, -0.9, -0.8, -0.7, -0.6, -0.5, -0.4, -0.3, -0.2, -0.1, 0.0, 0.1, 0.2, 0.3, 0.4, 0.5,
    0.6, 0.7, 0.8, 0.9, 1.0,
];

fn transitions(value: Vec<game_fighter::status::Transition>) -> Value {
    Value::Array(
        value
            .into_iter()
            .map(|t| {
                json!({
                    "from": t.from.name(),
                    "event": t.event,
                    "to": t.to.map(Phase::name),
                    "witnesses": t.witnesses,
                })
            })
            .collect(),
    )
}

fn main() {
    let catalog: Value = catalog::CATALOG
        .iter()
        .enumerate()
        .map(|(id, (action, file))| json!({ "id": id, "action": action, "file": file }))
        .collect();
    let phases: Value = Phase::ALL
        .iter()
        .map(|phase| json!({ "name": phase.name(), "grounded": phase.grounded() }))
        .collect();
    let phase_animation: Value = Phase::ALL
        .iter()
        .flat_map(|&phase| {
            let mut seen: Vec<Value> = Vec::new();
            for &axis in &AXIS_PROBES {
                let action = movement::pose_for_phase(phase, axis);
                if let Some(entry) =
                    seen.iter_mut().find(|e| e["action"].as_u64() == Some(action as u64))
                {
                    entry["axes"].as_array_mut().unwrap().push(json!(axis));
                } else {
                    seen.push(json!({ "phase": phase.name(), "action": action, "axes": [axis] }));
                }
            }
            seen
        })
        .collect();
    let output = json!({
        "catalog": catalog,
        "phases": phases,
        "phase_animation": phase_animation,
        "ground": transitions(ground_transitions()),
        "air": transitions(air_transitions()),
    });
    println!("{}", serde_json::to_string(&output).expect("serialize status export"));
}
