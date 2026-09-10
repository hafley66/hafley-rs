//! Executable status export. Emits the Rust-owned catalog membership/order,
//! the live Phase-to-animation selection, and per-phase chart membership that
//! `classification/8_status.mjs` joins with the retained ingest manifest.
//! Read-only: no files are written.
//!
//! Run: `cargo run --locked --offline -j2 --manifest-path smash/Cargo.toml \
//!   --example status_export`

use game_fighter::air::{AirEvent, AirFacts};
use game_fighter::ground::Event as GroundEvent;
use game_fighter::{Phase, air, ground};
use serde_json::{Value, json};
use smash::fighters::falcon::catalog;
use smash::fighters::falcon::movement::{self, SelectionFacts};

/// Stick sweep crossing both walk bands. The selection seam owns the
/// thresholds; this only samples both sides so a threshold change is visible
/// without editing the export.
const AXIS_PROBES: [f32; 3] = [0.0, 0.5, 1.0];
const BOOLS: [bool; 2] = [false, true];

/// Phases allowed to carry a catalog mapping with no executable ground or air
/// edge. Empty today: every host phase participates in a chart.
const CHART_EXEMPT: [&str; 0] = [];

fn ground_facts(bits: u8) -> ground::Facts {
    ground::Facts {
        dash: bits & 1 << 0 != 0,
        walk: bits & 1 << 1 != 0,
        forward: bits & 1 << 2 != 0,
        reverse: bits & 1 << 3 != 0,
        down: bits & 1 << 4 != 0,
        finished: bits & 1 << 5 != 0,
        stopped: bits & 1 << 6 != 0,
    }
}

/// Whether `ground::decide` emits any transition for this phase, evaluated over
/// the full local fact cube.
fn ground_member(phase: Phase) -> bool {
    if ground::decide(phase, GroundEvent::JumpRequest).is_some() {
        return true;
    }
    (0u8..128).any(|bits| {
        ground::decide(phase, GroundEvent::GroundIntent(ground_facts(bits))).is_some()
            || ground::decide(phase, GroundEvent::Motion(ground_facts(bits))).is_some()
    })
}

/// Whether `air::decide` emits any transition for this phase.
fn air_member(phase: Phase) -> bool {
    if air::decide(phase, AirEvent::Land).is_some() {
        return true;
    }
    BOOLS.iter().any(|&descending| {
        BOOLS.iter().any(|&jump_pressed| {
            (0u8..=2).any(|jumps_left| {
                air::decide(
                    phase,
                    AirEvent::Motion(AirFacts { descending, jump_pressed, jumps_left }),
                )
                .is_some()
            })
        })
    })
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
    let catalog_rows: Value = catalog::CATALOG
        .iter()
        .enumerate()
        .map(|(id, (action, file))| json!({ "id": id, "action": action, "file": file }))
        .collect();
    let phases: Value = Phase::ALL
        .iter()
        .map(|phase| json!({ "name": phase.name(), "grounded": phase.grounded() }))
        .collect();
    let chart_membership: Value = Phase::ALL
        .iter()
        .map(|phase| {
            json!({
                "phase": phase.name(),
                "ground": ground_member(*phase),
                "air": air_member(*phase),
            })
        })
        .collect();
    let output = json!({
        "catalog": catalog_rows,
        "phases": phases,
        "phase_animation": phase_animation(),
        "chart_membership": chart_membership,
        "chart_exempt": CHART_EXEMPT,
    });
    println!("{}", serde_json::to_string(&output).expect("serialize status export"));
}
