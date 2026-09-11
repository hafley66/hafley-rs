//! Machine-readable execution receipt for the pinned animation cases.

use game_fighter::{_6_qualification::animation_completion, Phase};
use serde_json::{Value, json};

const CASES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../classification/19_qualification_cases.json"
));

fn phase(name: &str) -> Phase {
    Phase::ALL
        .into_iter()
        .find(|phase| phase.name() == name)
        .unwrap_or_else(|| panic!("unknown runtime phase {name}"))
}

fn main() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let mut findings = Vec::new();
    for case in cases["callbacks"].as_array().unwrap() {
        let callback_id = case["source_callback_id"].as_str().unwrap();
        let runtime = &case["runtime"];
        let from = phase(runtime["phase"].as_str().unwrap());
        let finished = runtime["finished"].as_bool().unwrap();
        let forward = runtime["forward"].as_bool().unwrap();
        let expected = phase(runtime["destination"].as_str().unwrap());
        let observed = animation_completion(from, finished, forward);
        let unfinished = animation_completion(from, false, forward);
        let transition_passed = observed == Some(expected) && unfinished.is_none();
        let mut cloned = from;
        let mut restored: Phase =
            serde_json::from_slice(&serde_json::to_vec(&from).unwrap()).unwrap();
        let mut replay_passed = true;
        for (finished, forward) in [(false, forward), (true, forward)] {
            let expected = animation_completion(from, finished, forward);
            let clone_result = animation_completion(cloned, finished, forward);
            let restored_result = animation_completion(restored, finished, forward);
            replay_passed &= clone_result == expected && restored_result == expected;
            if let Some(next) = expected {
                cloned = next;
                restored = next;
            }
            replay_passed &= cloned == restored;
        }
        let callback_result = transition_passed && replay_passed;
        findings.push(json!({
            "requirementId": case["source_association_id"],
            "axis": "transition/callback mapping",
            "status": "unqualified",
            "result": {
                "source_callback_id": callback_id,
                "source_association_id": case["source_association_id"],
                "completion_call_id": case["completion_call_id"],
                "observed_destination": observed.map(Phase::name),
                "expected_destination": expected.name(),
                "unfinished_destination": unfinished.map(Phase::name),
                "clone_serde_suffix_replay": replay_passed,
                "runtime_comparison": if callback_result { "matched executable runtime but source context remains unmodeled" } else { "runtime execution failed" },
                "source_completion": case["source_completion"].clone(),
                "ordered_call_ids": case["ordered_calls"].as_array().unwrap().iter().map(|call| call["id"].clone()).collect::<Vec<_>>(),
            },
        }));
    }
    println!(
        "{}",
        json!({
            "schema": "games.animation-qualification-receipts.v1",
            "status": "unqualified",
            "findings": findings,
            "receipts": [],
        })
    );
}
