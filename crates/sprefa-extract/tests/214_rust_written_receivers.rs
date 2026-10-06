//! The 30 held method contracts, with the written-type opt-in.
#![cfg(feature = "rust-checker")]
use serde_json::Value;
use std::collections::BTreeMap;
use std::process::Command;
#[path = "support/0_rust_names_call_contract.rs"]
mod contract;

fn run(group: &str, flag: bool) -> String {
    let root = format!("tests/fixtures/{group}");
    let mut command = Command::new(env!("CARGO_BIN_EXE_ryii"));
    command
        .args(["--resolve", "--arms", "call", "--root", &root, &root])
        .env("RUST_LOG", "off")
        .env_remove("RYI_FAST_RECEIVERS");
    if flag {
        command.args(["--fast-receivers", "written"]);
    } else {
        command.env("RYI_FAST_RECEIVERS", "written");
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{group}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let facts = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    contract::call_contract(&facts)
}

#[test]
fn written_receivers_cover_the_thirty_method_contracts() {
    let cases = [
        ("selected reference Self and nested pattern scope", "rust_written_receivers"),
        ("t_116_rust_call_grind::method_init_hops_through_the_receiver_type", "rust_call_grind"),
        ("t_116_rust_call_grind::call_result_receiver_types_through_same_file_returns", "rust_call_grind"),
        ("t_116_rust_call_grind::cross_file_new_types_the_binding", "rust_call_grind"),
        ("t_130_rust_spelled_receiver::constructor_return_receiver_binds", "rust_spelled_receiver"),
        ("t_130_rust_spelled_receiver::spelled_unit_struct_receiver_binds", "rust_spelled_receiver"),
        ("t_130_rust_spelled_receiver::trait_bound_generic_receiver_binds", "rust_spelled_receiver"),
        ("t_130_rust_spelled_receiver::field_typed_receiver_binds", "rust_spelled_receiver"),
        ("t_135_untyped_receiver_rust::receiver_typed_call_binds", "rust_untyped_receiver"),
        ("t_135_untyped_receiver_rust::untyped_receiver_member_call_drops_inferred", "rust_untyped_receiver"),
        ("t_180_call_ladder::call_ladder_fast_and_slow", "call_ladder"),
        ("t_180_call_ladder::same_named_methods_on_local_types_keep_their_targets", "call_ladder"),
        ("t_190_rename_rust_slow::fast_tier_stops_on_a_method_call_whose_receiver_type_it_cannot_see", "rust_rename_slow"),
        ("t_68_rust_receivers::field_receiver_binds", "rust_findings/receivers"),
        ("t_68_rust_receivers::initializer_reads_the_outer_binding", "rust_findings/receivers"),
        ("t_68_rust_receivers::let_else_initializer_binds", "rust_findings/receivers"),
        ("t_68_rust_receivers::one_hop_through_result_binds", "rust_findings/receivers"),
        ("t_68_rust_receivers::let_typed_receiver_binds", "rust_findings/receivers"),
        ("t_68_rust_receivers::param_typed_receiver_binds_inherent_method", "rust_findings/receivers"),
        ("t_68_rust_receivers::self_return_one_hop_binds", "rust_findings/receivers"),
        ("t_68_rust_receivers::trait_impl_method_binds", "rust_findings/receivers"),
        ("t_68_rust_receivers::tuple_pattern_initializer_binds", "rust_findings/receivers"),
        ("t_68_rust_receivers::unknown_receiver_drops_inferred", "rust_findings/receivers"),
        ("t_71_rust_paths::inherent_impl_beats_trait_impl", "rust_findings/paths3"),
        ("t_71_rust_paths::trait_impl_binds_only_when_the_trait_is_in_scope", "rust_findings/paths3"),
        ("t_71_rust_paths::two_in_scope_traits_stay_ambiguous", "rust_findings/paths3"),
        ("t_72_rust_traits::bound_generic_receiver_binds_the_trait_fn_def", "rust_findings/traits"),
        ("t_72_rust_traits::dyn_trait_receiver_binds_the_trait_fn_def", "rust_findings/traits"),
        ("t_75_rust_trait_blob::trait_edge_targets_the_callers_own_declaration", "rust_findings/trait_blob"),
        ("t_72_rust_traits::trait_default_body_binds_the_unoverridden_method", "rust_findings/traits"),
        ("t_78_rust_checker::the_syntax_leg_alone_drops_the_inferred_receiver", "rust_findings/checker"),
    ];
    let expected: BTreeMap<String, String> = serde_json::from_str(include_str!(
        "fixtures/rust_written_receivers/0_contracts.json"
    ))
    .unwrap();
    let mut answers = BTreeMap::new();
    for (case, group) in cases {
        let actual = answers.entry(group).or_insert_with(|| {
            let flag = run(group, true);
            assert_eq!(flag, run(group, false), "flag/env {group}");
            flag
        });
        assert_eq!(
            actual,
            expected.get(group).expect("written contract fixture"),
            "{case}"
        );
    }
}
