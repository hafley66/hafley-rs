//! Executable qualification for four pinned ftCommon animation callbacks.
//!
//! The JSON case artifact is derived from the source inventory and retains the
//! callback identity plus every ordered direct-call identity. These assertions
//! execute the public game-fighter decision seam and replay the transition from
//! both a clone and a serde-restored phase.

use game_fighter::{Phase, qualification::animation_completion};
use serde_json::Value;

const CASES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../classification/19_qualification_cases.json"
));
const INVENTORY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../classification/12_source_inventory.json"
));

fn phase(name: &str) -> Phase {
    Phase::ALL
        .into_iter()
        .find(|phase| phase.name() == name)
        .unwrap_or_else(|| panic!("unknown runtime phase {name}"))
}

#[test]
fn pinned_cases_preserve_source_ids_and_execute_all_four_transitions() {
    let cases: Value = serde_json::from_str(CASES).unwrap();
    let inventory: Value = serde_json::from_str(INVENTORY).unwrap();
    assert_eq!(cases["source_revision"], inventory["revision"]);
    let rows = cases["callbacks"].as_array().unwrap();
    assert_eq!(rows.len(), 4);

    for case in rows {
        let callback_id = case["source_callback_id"].as_str().unwrap();
        let callback = inventory["callbacks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == callback_id)
            .unwrap_or_else(|| panic!("missing source callback {callback_id}"));
        assert_eq!(case["source"], callback["source"]);
        let association_id = case["source_association_id"].as_str().unwrap();
        assert!(inventory["associations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["id"] == association_id && row["callback"] == case["source"]["symbol"]));

        let calls = case["ordered_calls"].as_array().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0]["ordinal"], 0);
        assert_eq!(calls[1]["ordinal"], 1);
        assert_eq!(case["completion_call_id"], calls[0]["id"]);
        for (actual, expected) in calls.iter().zip(callback["calls"].as_array().unwrap()) {
            assert_eq!(actual["id"], expected["id"]);
            assert_eq!(actual["ordinal"], expected["ordinal"]);
            assert_eq!(actual["symbol"], expected["symbol"]);
            assert_eq!(actual["source"], expected["source"]);
        }

        let runtime = &case["runtime"];
        let from = phase(runtime["phase"].as_str().unwrap());
        let finished = runtime["finished"].as_bool().unwrap();
        let forward = runtime["forward"].as_bool().unwrap();
        let expected = phase(runtime["destination"].as_str().unwrap());
        assert_eq!(animation_completion(from, false, forward), None, "{} unfinished", callback_id);
        assert_eq!(animation_completion(from, finished, forward), Some(expected), "{} finished", callback_id);
        assert_eq!(case["source_completion"]["qualification"], "unqualified");
        assert!(!case["source_completion"]["reason"].as_str().unwrap().is_empty());

        let mut cloned = from;
        let mut restored: Phase = serde_json::from_slice(&serde_json::to_vec(&from).unwrap()).unwrap();
        for (finished, forward) in [(false, forward), (true, forward)] {
            let expected = animation_completion(from, finished, forward);
            let clone_result = animation_completion(cloned, finished, forward);
            let restored_result = animation_completion(restored, finished, forward);
            assert_eq!(clone_result, expected, "clone replay {callback_id}");
            assert_eq!(restored_result, expected, "serde replay {callback_id}");
            if let Some(next) = expected {
                cloned = next;
                restored = next;
            }
            assert_eq!(cloned, restored, "suffix state {callback_id}");
        }
    }
}
