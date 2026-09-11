//! Executable status export. Emits the observed Rust-owned facts that
//! `classification/8_status.mjs` joins against authored TypeSpec and retained
//! ingest records. Read-only: no files are written.
//!
//! Run: `cargo run --locked --offline -j2 --manifest-path smash/Cargo.toml \
//!   --example status_export`

use game_fighter::Phase;
use game_fighter::status::{air_transitions, ground_transitions};
use serde_json::{Value, json};
use smash::fighters::pigeon::catalog;
use smash::fighters::pigeon::movement::{self, SelectionFacts};

/// Deterministic stick sweep; the phase->animation selection seam is sampled
/// only at these points, so band changes appear as separate observed actions.
const AXIS_PROBES: [f32; 21] = [
    -1.0, -0.9, -0.8, -0.7, -0.6, -0.5, -0.4, -0.3, -0.2, -0.1, 0.0, 0.1, 0.2, 0.3, 0.4, 0.5,
    0.6, 0.7, 0.8, 0.9, 1.0,
];
const BOOLS: [bool; 2] = [false, true];

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

/// Enumerate the runtime selection over the full fact cube. Entries are
/// executable outputs of `movement::select`, grouped by phase/action/condition;
/// no mapping is authored here.
fn phase_animation() -> Value {
    let mut rows: Vec<Value> = Vec::new();
    for &phase in &Phase::ALL {
        for &axis in &AXIS_PROBES {
            for &attacking in &BOOLS {
                for &attack_pressed in &BOOLS {
                    for &landed in &BOOLS {
                        for &recovering in &BOOLS {
                            for &landing_lag in &BOOLS {
                                let facts = SelectionFacts {
                                    attacking,
                                    attack_pressed,
                                    landed,
                                    recovering,
                                    landing_lag,
                                };
                                let (action, condition) = movement::select(phase, axis, facts);
                                let key = condition.key();
                                if let Some(entry) = rows.iter_mut().find(|entry| {
                                    entry["phase"] == phase.name()
                                        && entry["action"].as_u64() == Some(action as u64)
                                        && entry["condition"] == key
                                }) {
                                    entry["axes"].as_array_mut().unwrap().push(json!(axis));
                                } else {
                                    rows.push(json!({
                                        "phase": phase.name(),
                                        "action": action,
                                        "condition": key,
                                        "axes": [axis],
                                    }));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Value::Array(rows)
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
    let output = json!({
        "catalog": catalog,
        "phases": phases,
        "phase_animation": phase_animation(),
        "ground": transitions(ground_transitions()),
        "air": transitions(air_transitions()),
    });
    println!("{}", serde_json::to_string(&output).expect("serialize status export"));
}
