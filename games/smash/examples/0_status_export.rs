//! Executable status export. Emits the observed Rust-owned facts that
//! `classification/8_status.mjs` joins against authored TypeSpec and retained
//! ingest records. Read-only: no files are written.
//!
//! Run: `cargo run --locked --offline -j2 --manifest-path smash/Cargo.toml \
//!   --example status_export`

use game_fighter::Phase;
use game_fighter::_5_status::{RuntimeInventory, runtime_inventory};
use serde_json::{Value, json};
use smash::fighters::dog;
use smash::fighters::pigeon::{self, movement::{self, SelectionFacts}};
use std::sync::Arc;

/// Deterministic stick sweep; the phase->animation selection seam is sampled
/// only at these points, so band changes appear as separate observed actions.
const AXIS_PROBES: [f32; 21] = [
    -1.0, -0.9, -0.8, -0.7, -0.6, -0.5, -0.4, -0.3, -0.2, -0.1, 0.0, 0.1, 0.2, 0.3, 0.4, 0.5,
    0.6, 0.7, 0.8, 0.9, 1.0,
];
const BOOLS: [bool; 2] = [false, true];

fn transitions(value: Vec<game_fighter::_5_status::Transition>) -> Value {
    Value::Array(
        value
            .into_iter()
            .map(|t| {
                json!({
                    "from": t.from.name(),
                    "event": t.event,
                    "to": t.to.map(Phase::name),
                    "callback": {
                        "domain": t.callback.domain,
                        "state": t.callback.state.name(),
                        "event": t.callback.event,
                    },
                    "witnesses": t.witnesses,
                    "fact_bits": t.fact_bits,
                })
            })
            .collect(),
    )
}

/// Enumerate the runtime selection over the full fact cube. Entries are
/// executable outputs of `movement::select`, grouped by phase/action/condition;
/// no mapping is authored here.
fn phase_animation(states: &[Phase]) -> Value {
    let mut rows: Vec<Value> = Vec::new();
    for &phase in states {
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

/// Reshape the pure fighter inventory for the machine-readable export. Pigeon
/// bindings are the observed outputs of its existing pure `movement::select`
/// seam, including conditional action selections.
fn runtime_inventory_value(inventory: RuntimeInventory) -> Value {
    let pigeon_selection_capability = phase_animation(&inventory.states);
    let pigeon_bindings = pigeon_live_bindings();
    json!({
        "states": inventory.states.iter().map(|phase| json!({
            "name": phase.name(),
            "grounded": phase.grounded(),
        })).collect::<Vec<_>>(),
        "ground": transitions(inventory.ground),
        "air": transitions(inventory.air),
        "callbacks": inventory.callbacks,
        "events": inventory.events,
        "effects": inventory.effects,
        "pigeon_bindings": pigeon_bindings,
        "pigeon_selection_capability": pigeon_selection_capability,
    })
}

/// Execute the existing Pigeon locomotion controller over its deterministic
/// reachability tape and retain the action/phase pairs it actually selects.
/// Selector fact-cube coverage is exported separately as a capability.
fn pigeon_live_bindings() -> Value {
    let actions: Arc<[pigeon::Action]> = pigeon::catalog::load_baked()
        .expect("committed Pigeon baked actions")
        .into();
    let mut simulation = pigeon::Simulation::new_locomotion(actions, false);
    let mut rows = Vec::new();
    for tick in 0..360 {
        let buttons = match tick {
            35..=42 | 150..=156 | 270 => 1,
            55 | 175 => 2,
            220..=225 => 4,
            _ => 0,
        };
        let axis = match tick {
            0..=75 => 1.0,
            100..=180 => -1.0,
            240..=300 => 0.4,
            _ => 0.0,
        };
        let world = simulation.advance_controlled(buttons, axis);
        let phase = world
            .movement
            .as_ref()
            .expect("Pigeon locomotion controller state")
            .phase;
        if !rows.iter().any(|row: &Value| {
            row["phase"] == phase.name() && row["action"].as_u64() == Some(world.action as u64)
        }) {
            rows.push(json!({ "phase": phase.name(), "action": world.action }));
        }
    }
    Value::Array(rows)
}

fn main() {
    // The committed generator output owns catalog membership and order; this
    // export only reshapes it for the status join and never re-derives it.
    let evidence: Value =
        serde_json::from_str(include_str!("../src/fighters/pigeon/generated/5_catalog.json"))
            .expect("parse committed pigeon catalog evidence");
    let catalog: Value = evidence["entries"]
        .as_array()
        .expect("catalog entries")
        .iter()
        .map(|entry| json!({ "id": entry["id"], "action": entry["name"], "file": entry["file"] }))
        .collect();
    let inventory = runtime_inventory();
    let phases: Value = inventory.states
        .iter()
        .map(|phase| json!({ "name": phase.name(), "grounded": phase.grounded() }))
        .collect();
    let runtime = runtime_inventory_value(inventory);
    let output = json!({
        "catalog": catalog,
        "phases": phases,
        "phase_animation": runtime["pigeon_selection_capability"].clone(),
        "pigeon_live_bindings": runtime["pigeon_bindings"].clone(),
        "ground": runtime["ground"].clone(),
        "air": runtime["air"].clone(),
        "runtime_inventory": runtime,
        "dog": dog_evidence(),
    });
    println!("{}", serde_json::to_string(&output).expect("serialize status export"));
}

/// Execute the source-free Dog slice over its deterministic tape and report the
/// phase/action pairs the shared controller actually produced. Actions and
/// phases the runtime never selected are listed explicitly; a generated role
/// binding alone does not mark an action live.
fn dog_evidence() -> Value {
    let mut simulation = dog::Simulation::new().expect("Dog baked actions");
    let observed = dog::simulation::observe_tape(&mut simulation);

    let mut rows: Vec<Value> = Vec::new();
    for (phase, action) in &observed {
        if rows.iter().any(|row| row["action"].as_u64() == Some(*action as u64)) {
            continue;
        }
        rows.push(json!({ "action": action, "phase": phase }));
    }

    let reached: Vec<&str> = observed.iter().map(|(phase, _)| *phase).collect();
    let unreached_phases: Vec<&str> = Phase::ALL
        .iter()
        .map(|phase| phase.name())
        .filter(|name| !reached.contains(name))
        .collect();
    let selected: Vec<usize> = observed.iter().map(|(_, action)| *action).collect();
    let unselected_actions: Vec<usize> = (0..dog::catalog::ACTION_COUNT)
        .filter(|action| !selected.contains(action))
        .collect();

    json!({
        "observed": rows,
        "unreached_phases": unreached_phases,
        "unselected_actions": unselected_actions,
    })
}
