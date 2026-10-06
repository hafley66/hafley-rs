# Names full-suite failure triage

| Primary cause | Failures | Action |
| --- | ---: | --- |
| (a) Fixture has no owning Cargo.toml | 50 | Add minimal Cargo ownership; retain expectations. |
| (b) Dot method name guess or old inferred reason | 30 | Only method expectations may change to needs_types; retain static-call expectations. |
| (c) Provider / adapter source regression | 17 | Fix source first; retain expectations. |
| (d) Other, including mixed or pending diagnosis | 18 | Require evidence before any expectation change. |

Counts assign each failed test once by its first failing assertion. Some fixtures in (b) also lack Cargo ownership, which must be added. Mixed tests retain non-method assertions. Category (d) includes unresolved diagnoses and does not authorize snapshot changes.

## a

- `t_171_reach_entry::soopy_lib_reaches_every_lib_file`

- `t_115_rust_type_grind::an_impl_self_type_declared_elsewhere_references_that_file`
- `t_115_rust_type_grind::an_impl_self_type_in_its_declaring_file_references_the_declaration`
- `t_116_rust_call_grind::struct_literal_keeps_its_row`
- `t_146_rename_stop_lines::the_two_route_file_exits_zero`
- `t_147_rename_path_union::a_file_with_one_route_plans_exactly_as_before`
- `t_147_rename_path_union::a_symbol_two_path_attrs_reach_gets_a_plan_instead_of_a_stop`
- `t_147_rename_path_union::an_occurrence_both_routes_reach_is_planned_once`
- `t_147_rename_path_union::the_plan_is_the_union_of_both_routes`
- `t_135_untyped_receiver_rust::free_call_keeps_name_match`
- `t_180_call_ladder::qualified_new_uses_its_declaring_type`
- `t_1_resolve_cli::resolve_names_a_closure_caller`
- `t_26_parallel_dispatch::single_thread_matches_default_cap`
- `t_49_rust_resolve_scaling::same_file_helper_still_wins_after_the_hoist`
- `t_52_rust_crawl_kinks::a_closure_caller_edge_mirrors_onto_the_enclosing_fn`
- `t_52_rust_crawl_kinks::a_dropped_site_mints_an_unresolved_row_naming_why`
- `t_52_rust_crawl_kinks::a_module_qualified_call_binds_in_the_module_the_path_names`
- `t_52_rust_crawl_kinks::a_type_qualifier_keeps_the_name_leg_and_an_unknown_crate_mints_nothing`
- `t_52_rust_crawl_kinks::const_block_call_resolves_to_its_sibling`
- `t_52_rust_crawl_kinks::initializer_calls_carry_the_const_or_static_item_as_caller`
- `t_52_rust_crawl_kinks::one_mirror_edge_per_closure_caller_edge`
- `t_60_rust_corpus_scope::same_file_scope_beats_foreign_top_level_def`
- `t_68_rust_receivers::a_field_named_like_a_type_does_not_capture_the_reference`
- `t_68_rust_receivers::self_assoc_binds`
- `t_68_rust_receivers::single_glob_source_binds`
- `t_68_rust_receivers::type_assoc_binds`
- `t_72_rust_traits::external_module_qualified_prefix_drops_external`
- `t_72_rust_traits::impl_fn_beats_the_trait_fallback`
- `t_76_rust_variant_names::a_variant_does_not_capture_a_same_named_free_fn`
- `t_72_rust_traits::trait_assoc_call_binds_the_trait_fn_def`
- `t_72_rust_traits::zero_impl_assoc_call_binds_the_trait_default`
- `t_76_rust_variant_names::variant_ctor_names_the_variant`
- `t_76_rust_variant_names::variant_ctor_through_self_names_the_variant`
- `t_77_rust_collapsed_span::a_clean_span_in_the_same_file_still_binds`
- `t_79_rust_generic_args::bound_generic_argument_is_named`
- `t_79_rust_generic_args::impl_self_type_head_and_generic_argument_are_named`
- `t_79_rust_generic_args::impl_trait_generic_argument_is_named`
- `t_79_rust_qualified_type::qualified_field_type_binds_its_declaration`
- `t_79_rust_qualified_type::qualified_generic_argument_binds`
- `t_79_rust_type_alias::alias_generic_argument_is_named`
- `t_79_rust_type_alias::alias_is_an_edge_destination`
- `t_79_rust_type_alias::alias_names_its_right_hand_side`
- `t_79_rust_variant_payload::variant_payload_generic_argument_is_named`
- `t_79_rust_variant_payload::variant_payload_types_are_enum_edges`
- `t_80_rust_impl_owner::impl_owner_is_owned_by_the_impls_file`
- `t_80_rust_impl_owner::impl_trait_for_a_type_declared_elsewhere_binds_the_trait`
- `t_80_rust_impl_owner::impl_self_type_generic_argument_binds`
- `t_91_origin_column::rust_same_file_edge_says_same_file`
- `t_15_typegraph_d2::scc_layers_keep_hops_cycles_and_cross_board_edges_deterministic`
- `v5_parity::deferred_jsx::deferred_jsx_props_captures_and_callers`

## b

- `t_116_rust_call_grind::method_init_hops_through_the_receiver_type`
- `t_116_rust_call_grind::call_result_receiver_types_through_same_file_returns`
- `t_116_rust_call_grind::cross_file_new_types_the_binding`
- `t_130_rust_spelled_receiver::constructor_return_receiver_binds`
- `t_130_rust_spelled_receiver::spelled_unit_struct_receiver_binds`
- `t_130_rust_spelled_receiver::trait_bound_generic_receiver_binds`
- `t_130_rust_spelled_receiver::field_typed_receiver_binds`
- `t_135_untyped_receiver_rust::receiver_typed_call_binds`
- `t_135_untyped_receiver_rust::untyped_receiver_member_call_drops_inferred`
- `t_180_call_ladder::call_ladder_fast_and_slow`
- `t_180_call_ladder::same_named_methods_on_local_types_keep_their_targets`
- `t_190_rename_rust_slow::fast_tier_stops_on_a_method_call_whose_receiver_type_it_cannot_see`
- `t_68_rust_receivers::field_receiver_binds`
- `t_68_rust_receivers::initializer_reads_the_outer_binding`
- `t_68_rust_receivers::let_else_initializer_binds`
- `t_68_rust_receivers::one_hop_through_result_binds`
- `t_68_rust_receivers::let_typed_receiver_binds`
- `t_68_rust_receivers::param_typed_receiver_binds_inherent_method`
- `t_68_rust_receivers::self_return_one_hop_binds`
- `t_68_rust_receivers::trait_impl_method_binds`
- `t_68_rust_receivers::tuple_pattern_initializer_binds`
- `t_68_rust_receivers::unknown_receiver_drops_inferred`
- `t_71_rust_paths::inherent_impl_beats_trait_impl`
- `t_71_rust_paths::trait_impl_binds_only_when_the_trait_is_in_scope`
- `t_71_rust_paths::two_in_scope_traits_stay_ambiguous`
- `t_72_rust_traits::bound_generic_receiver_binds_the_trait_fn_def`
- `t_72_rust_traits::dyn_trait_receiver_binds_the_trait_fn_def`
- `t_75_rust_trait_blob::trait_edge_targets_the_callers_own_declaration`
- `t_72_rust_traits::trait_default_body_binds_the_unoverridden_method`
- `t_78_rust_checker::the_syntax_leg_alone_drops_the_inferred_receiver`

## c

- `dogfood_reexport::public_reexport_routes_survive_a_cleave_into_a_private_module`
- `t_0_sqlite::large_fast_stream_matches_project_row_order`
- `t_166_cleave_rust::undeclared_destinations_stop_before_rewriting_callers`
- `t_166_cleave_rust::batch_keeps_every_row_source_parseable_while_composing`
- `t_171_reach_entry::rust_entries_follow_mod_path_attr_and_own_crate_paths`
- `t_173_move_cross_crate::a_batch_carries_a_module_and_its_child_across_crates`
- `t_175_cleave_ladder::cleave_ladder`
- `t_180_call_ladder::self_struct_constructors_bind_to_the_enclosing_impl_type`
- `t_57_rust_module_plane::a_crate_qualified_use_binds_through_the_plane`
- `t_57_rust_module_plane::a_cross_crate_use_binds_when_the_target_crate_is_in_the_corpus`
- `t_57_rust_module_plane::a_path_attribute_module_binds_to_its_override`
- `t_57_rust_module_plane::a_renamed_use_binds_by_source_name_under_the_local_alias`
- `t_57_rust_module_plane::a_super_qualified_use_binds_through_the_plane`
- `t_57_rust_module_plane::edge_count_matches_the_fixtures_written_bindings`
- `t_57_rust_module_plane::two_globs_offering_the_same_name_drop_ambiguous`
- `t_71_rust_paths::external_module_prefixes_drop_external`
- `t_8_scip_families_cli::the_discrimination_holds_through_rust_analyzer_too`

## d

- `t_130_rust_spelled_receiver::shadowed_call_does_not_bind_free_fn`
- `t_166_cleave_rust::an_unloadable_manifest_stops_the_rust_cleave_with_the_reason`
- `t_169_graph_uses_rust::graph_uses_follows_corpus_crate_reexports_and_declines_registry_crates`
- `t_170_ratchet_sites_rust::fast_matches_slow_on_soopy_at_the_pinned_rate`
- `t_170_ratchet_sites_rust::fast_type_edges_match_slow_on_soopy_at_the_pinned_rate`
- `t_173_rust_export_cycle::a_reexport_cycle_through_a_use_bound_head_terminates`
- `t_174_type_ladder::cargo_package_and_qualified_type_boundaries`
- `t_174_type_ladder::type_ladder_fast_and_slow`
- `t_174_type_ladder::type_scope_ladder_preserves_declared_fixture_dependency`
- `t_177_crate_scope::a_bare_name_binds_only_inside_the_crate_and_its_dependencies`
- `t_178_cli_http_parity::cli_router_and_unix_socket_share_the_contract`
- `t_176_rename_ladder::every_ladder_rename_compiles`
- `t_178_ryi_help::generated_format_accepts_root_and_global_positions`
- `t_180_call_ladder::cargo_target_and_dependency_call_boundaries`
- `t_180_call_ladder::contextual_default_uses_its_type_impl`
- `t_57_rust_module_plane::two_pub_use_hops_resolve_with_hops_two`
- `t_71_rust_paths::prelude_trait_counts_as_in_scope`
- `t_golden_parity::call_resolve_scip_ratchet_rust`

## Reviewed category (a)/(d) output changes

| Test | Evidence | Change |
| --- | --- | --- |
| t_135 free calls; t_91 Rust origin | Same physical push/mk/helper targets; Names provider supplied the answer | same_file -> module_plane |
| t_26 parallel dispatch | Three call edges plus three written import rows after fixture imports | Count call edges; retain full-output equality |
| t_52 unknown qualified head | other_crate is absent from Cargo dependencies and RA module scopes | no_corpus_def instead of a corpus-name-count ambiguity |
| t_130 shadowed callable | project is a function parameter, with no module-definition answer | needs_types; no free-fn edge |
| t_177 crate scope | only_a_fn/shared_fn/OnlyA/Shared lack imports in app; lib_a::shared_fn is written | Keep qualified call and local Twice; remove unimported guesses |
| t_176 rename merge | types::A resolves through RA to workspace A; both edited fixtures compile | Six edits / one skipped macro site with either SCIP flag |
| Rust SCIP ratchet | All five physical destinations agree with SCIP | Combined original 2+3 floor retained as module_plane=5 |

Category (b) expectation drafts are held outside git in the ignored bench directory.
The mixed closure golden retains its old method reasons until that decision.

Latest targeted results before the final transport build: (a) 49/50, with the mixed
closure method reasons held; all 17 (c) gates pass; 9/10 targeted (d) source gates
pass. The remaining tenth is contextual Default::default. Root lifecycle, index
coordinate joins, and Native/tree module-facts fixture tables pass.

The two std-dependent static-call cases are folded into the table-driven
`std_associated_calls_names_and_types` fixture test. Names retains the explicit
Defaults constructor but declines both unqualified Default::default calls; Types
selects the workspace impl at _0_left.rs:145. Gem::from declines in Names because
its std From trait is outside the loaded database; Types selects gem.rs:102.
Both Types destinations were observed before updating these category (d) cases.

Final independent targeted gates: category (a) 49/50, category (c) 17/17, category
(d) 17/18 (the two std cases share one passing table). The remaining (a) closure
case and (d) call ratchet contain the held receiver policy. All 30 category (b)
expectations retain their original method-call behavior pending the user decision.
t_186 and t_207 pass. Transport CLI/router/socket and no-child bodies match.
Soopy Type ratchet: 1041 shared, 3 fast-only, 1 slow-only. Call ratchet: 663 true
positives, zero wrong/overbound, 143 needs_types misses, 7 unresolved misses.

## Applied default receiver decision (2026-10-06)

User decision (1) authorizes all 30 category (b) expectations above to change to
`needs_types`. Each listed case now belongs to its fixture's whole-call table:
116 (local/cross), 130 (spelled), 135 (untyped), 68 (lib/multi), 71 (paths),
72 (traits), and 75 (two trait declarations). The two 180 cases retain their
whole fast/slow table and pin both local `rows` drops; 78 pins the checker split;
190 pins the rename stop diagnostic. Static calls retain their physical targets.
The mixed closure golden changes iter/map/collect to `needs_types`.

Soopy's call floor changes from 748/1/2/64 to 663/0/0/150 (true positives,
wrong target, overbound, misses). The 150 misses comprise 143 `needs_types` and
7 unresolved sites. This is the baseline for the separate written-receiver
opt-in recovery measurement. The Type floor remains unchanged.

| Previous test | Default contract |
| --- | --- |
| `t_116_rust_call_grind::call_result_receiver_types_through_same_file_returns` | Method sites abstain `needs_types`; written static targets retained |
| `t_116_rust_call_grind::cross_file_new_types_the_binding` | Method sites abstain `needs_types`; written static targets retained |
| `t_116_rust_call_grind::method_init_hops_through_the_receiver_type` | Method sites abstain `needs_types`; written static targets retained |
| `t_130_rust_spelled_receiver::constructor_return_receiver_binds` | Method sites abstain `needs_types`; written static targets retained |
| `t_130_rust_spelled_receiver::field_typed_receiver_binds` | Method sites abstain `needs_types`; written static targets retained |
| `t_130_rust_spelled_receiver::spelled_unit_struct_receiver_binds` | Method sites abstain `needs_types`; written static targets retained |
| `t_130_rust_spelled_receiver::trait_bound_generic_receiver_binds` | Method sites abstain `needs_types`; written static targets retained |
| `t_135_untyped_receiver_rust::receiver_typed_call_binds` | Method sites abstain `needs_types`; written static targets retained |
| `t_135_untyped_receiver_rust::untyped_receiver_member_call_drops_inferred` | Method sites abstain `needs_types`; written static targets retained |
| `t_180_call_ladder::call_ladder_fast_and_slow` | Method sites abstain `needs_types`; written static targets retained |
| `t_180_call_ladder::same_named_methods_on_local_types_keep_their_targets` | Method sites abstain `needs_types`; written static targets retained |
| `t_190_rename_rust_slow::fast_tier_stops_on_a_method_call_whose_receiver_type_it_cannot_see` | Method sites abstain `needs_types`; written static targets retained |
| `t_68_rust_receivers::field_receiver_binds` | Method sites abstain `needs_types`; written static targets retained |
| `t_68_rust_receivers::initializer_reads_the_outer_binding` | Method sites abstain `needs_types`; written static targets retained |
| `t_68_rust_receivers::let_else_initializer_binds` | Method sites abstain `needs_types`; written static targets retained |
| `t_68_rust_receivers::let_typed_receiver_binds` | Method sites abstain `needs_types`; written static targets retained |
| `t_68_rust_receivers::one_hop_through_result_binds` | Method sites abstain `needs_types`; written static targets retained |
| `t_68_rust_receivers::param_typed_receiver_binds_inherent_method` | Method sites abstain `needs_types`; written static targets retained |
| `t_68_rust_receivers::self_return_one_hop_binds` | Method sites abstain `needs_types`; written static targets retained |
| `t_68_rust_receivers::trait_impl_method_binds` | Method sites abstain `needs_types`; written static targets retained |
| `t_68_rust_receivers::tuple_pattern_initializer_binds` | Method sites abstain `needs_types`; written static targets retained |
| `t_68_rust_receivers::unknown_receiver_drops_inferred` | Method sites abstain `needs_types`; written static targets retained |
| `t_71_rust_paths::inherent_impl_beats_trait_impl` | Method sites abstain `needs_types`; written static targets retained |
| `t_71_rust_paths::trait_impl_binds_only_when_the_trait_is_in_scope` | Method sites abstain `needs_types`; written static targets retained |
| `t_71_rust_paths::two_in_scope_traits_stay_ambiguous` | Method sites abstain `needs_types`; written static targets retained |
| `t_72_rust_traits::bound_generic_receiver_binds_the_trait_fn_def` | Method sites abstain `needs_types`; written static targets retained |
| `t_72_rust_traits::dyn_trait_receiver_binds_the_trait_fn_def` | Method sites abstain `needs_types`; written static targets retained |
| `t_72_rust_traits::trait_default_body_binds_the_unoverridden_method` | Method sites abstain `needs_types`; written static targets retained |
| `t_75_rust_trait_blob::trait_edge_targets_the_callers_own_declaration` | Method sites abstain `needs_types`; written static targets retained |
| `t_78_rust_checker::the_syntax_leg_alone_drops_the_inferred_receiver` | Method sites abstain `needs_types`; written static targets retained |

The final suite found five additional failures. The path-include cleave guard
now permits a sibling source/destination pair with an engine-provided declarer;
the outer closure invocation declines instead of borrowing its nested callee;
rename diagnostics pin both typed and untyped receiver stops. The two-glob
fixture now writes both glob imports. The kind-vocabulary golden adds exactly
the two written module/import specifier rows appended to the closure fixture,
with all existing fixture rows byte-identical.

The missing-origin ratchet assertion now charges module_plane floor 5, matching
the retained combined floor; its missing-origin failure remains asserted.

The wire golden also mirrors the changed JSONL fixture itself: its data documents
now encode two module/import rows and the selected needs_types reasons. The
replacement matched exactly one old fixture block (56 -> 76 data facts).

Validation: the full suite ran once, 1251 passed / six failed / 19 ignored
(plus library 7/7 and binary 18/18). Each of the six failures was repaired and
rerun individually; all pass. The updated t_71 whole-call table also passes.
No other full-suite failures occurred. No second corpus agreement was run.
