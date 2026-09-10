//! Executable status export. Emits the Rust-owned catalog membership/order and
//! the live Phase-to-animation selection that `classification/8_status.mjs`
//! joins with the retained ingest manifest. Read-only: no files are written.
//!
//! Run: `cargo run --locked --offline -j2 --manifest-path smash/Cargo.toml \
//!   --example status_export`

use game_fighter::Phase;
use serde_json::{Value, json};
use smash::fighters::falcon::{catalog, movement};

/// Stick sweep crossing both walk bands. The Phase-to-action seam owns the
/// thresholds; this only samples both sides of them, so a threshold change is
/// visible in the export without editing it.
const AXIS_PROBES: [f32; 3] = [0.0, 0.5, 1.0];

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
        "catalog": catalog_rows,
        "phases": phases,
        "phase_animation": phase_animation,
    });
    println!("{}", serde_json::to_string(&output).expect("serialize status export"));
}
