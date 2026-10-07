//! Every integration test in one binary: cargo links once, not per file.
//! Run one: cargo nextest run -p sprefa-extract -E 'test(/type_ladder/)'

#[path = "214_rust_written_receivers.rs"]
mod t_214_rust_written_receivers;

#[path = "213_dogfood_list_json.rs"]
mod dogfood_list_json;
#[path = "212_dogfood_reexport.rs"]
mod dogfood_reexport;
#[path = "211_dogfood_merge.rs"]
mod dogfood_merge;

#[path = "210_dogfood_rename.rs"]
mod t_210_dogfood_rename;

#[path = "209_dogfood_workspace.rs"]
mod t_209_dogfood_workspace;

#[path = "208_dogfood_orphans.rs"]
mod t_208_dogfood_orphans;

#[path = "207_dogfood_callers.rs"]
mod t_207_dogfood_callers;

#[cfg(feature = "cli")]
#[path = "support/0_daemon_guard.rs"]
mod daemon_guard;
#[cfg(feature = "cli")]
#[path = "support/1b_command.rs"]
mod command_support;
#[cfg(feature = "cli")]
#[path = "support/7_daemon.rs"]
mod daemon_support;
#[cfg(feature = "typescript")]
#[path = "support/0a_stock_tsgo.rs"]
mod stock_tsgo;
#[path = "support/1_v6_only.rs"]
mod v6_only;

#[path = "0_prolog.rs"]
mod t_0_prolog;
#[path = "0_sqlite.rs"]
mod t_0_sqlite;
#[path = "100_rust_call_scm_production.rs"]
mod t_100_rust_call_scm_production;
#[path = "100_tsi_intersection.rs"]
mod t_100_tsi_intersection;
#[path = "101_rust_const_init_collection.rs"]
mod t_101_rust_const_init_collection;
#[path = "101_ts_semantic_tsi.rs"]
mod t_101_ts_semantic_tsi;
#[path = "102_rust_semantic_tsi.rs"]
mod t_102_rust_semantic_tsi;
#[path = "103_trail.rs"]
mod t_103_trail;
#[path = "104_tier_decline_diagnostic.rs"]
mod t_104_tier_decline_diagnostic;
#[path = "105_resolve_syntax_tsi.rs"]
mod t_105_resolve_syntax_tsi;
#[path = "106_rust_syntax_graph.rs"]
mod t_106_rust_syntax_graph;
#[path = "107_rust_checker_features.rs"]
mod t_107_rust_checker_features;
#[path = "108_rust_checker_walk_by_file.rs"]
mod t_108_rust_checker_walk_by_file;
#[path = "109_rust_checker_site_cost.rs"]
mod t_109_rust_checker_site_cost;
#[path = "10_source_tree.rs"]
mod t_10_source_tree;
#[path = "110_tsi_name.rs"]
mod t_110_tsi_name;
#[path = "111_cli_identity.rs"]
mod t_111_cli_identity;
#[path = "111_ts_syntax_graph.rs"]
mod t_111_ts_syntax_graph;
#[path = "112_build_metadata.rs"]
mod t_112_build_metadata;
#[path = "112_go_syntax_graph.rs"]
mod t_112_go_syntax_graph;
#[path = "113_ts_module_edges.rs"]
mod t_113_ts_module_edges;
#[path = "114_go_graph_grind.rs"]
mod t_114_go_graph_grind;
#[path = "115_rust_type_grind.rs"]
mod t_115_rust_type_grind;
#[path = "116_rust_call_grind.rs"]
mod t_116_rust_call_grind;
#[path = "117_python_syntax_graph.rs"]
mod t_117_python_syntax_graph;
#[path = "118_python_call_grind.rs"]
mod t_118_python_call_grind;
#[path = "119_kotlin_syntax_graph.rs"]
mod t_119_kotlin_syntax_graph;
#[path = "11_markdown.rs"]
mod t_11_markdown;
#[path = "124_cfg_python_prolog.rs"]
mod t_124_cfg_python_prolog;
#[path = "125_markdown_links_fences.rs"]
mod t_125_markdown_links_fences;
#[path = "126_python_modules.rs"]
mod t_126_python_modules;
#[path = "127_kotlin_modules.rs"]
mod t_127_kotlin_modules;
#[path = "128_scip_relationship_conforms.rs"]
mod t_128_scip_relationship_conforms;
#[path = "129_go_checker_tier.rs"]
mod t_129_go_checker_tier;
#[path = "129_scip_external_coverage.rs"]
mod t_129_scip_external_coverage;
#[path = "12_df_identity.rs"]
mod t_12_df_identity;
#[path = "130_rust_spelled_receiver.rs"]
mod t_130_rust_spelled_receiver;
#[path = "131_kotlin_module_resolve.rs"]
mod t_131_kotlin_module_resolve;
#[path = "133_go_binding_legs.rs"]
mod t_133_go_binding_legs;
#[path = "134_ts_binding_legs.rs"]
mod t_134_ts_binding_legs;
#[path = "135_untyped_receiver_rust.rs"]
mod t_135_untyped_receiver_rust;
#[path = "136_untyped_receiver_ts.rs"]
mod t_136_untyped_receiver_ts;
#[path = "137_kotlin_receiver_legs.rs"]
mod t_137_kotlin_receiver_legs;
#[path = "138_untyped_receiver_kotlin.rs"]
mod t_138_untyped_receiver_kotlin;
#[path = "13_flow_join.rs"]
mod t_13_flow_join;
#[path = "141_unresolved_contract.rs"]
mod t_141_unresolved_contract;
#[path = "142_lines_flag.rs"]
mod t_142_lines_flag;
#[path = "143_default_call_plane.rs"]
mod t_143_default_call_plane;
#[path = "145_query_predicate_scope.rs"]
mod t_145_query_predicate_scope;
#[path = "145a_query_scm_dogfood.rs"]
mod t_145a_query_scm_dogfood;
#[path = "146_rename_stop_lines.rs"]
mod t_146_rename_stop_lines;
#[path = "147_rename_path_union.rs"]
mod t_147_rename_path_union;
#[path = "148_rename_abstain_ts.rs"]
mod t_148_rename_abstain_ts;
#[path = "149_graph_callers_ts.rs"]
mod t_149_graph_callers_ts;
#[path = "14_df_identity_go_kotlin.rs"]
mod t_14_df_identity_go_kotlin;
#[path = "150_fast_scm_kotlin.rs"]
mod t_150_fast_scm_kotlin;
#[path = "155_cleave_ts.rs"]
mod t_155_cleave_ts;
#[path = "155a_cleave_ts_oracle.rs"]
mod t_155a_cleave_ts_oracle;
#[path = "156_cleave_play.rs"]
mod t_156_cleave_play;
#[path = "156_stratify.rs"]
mod t_156_stratify;
#[path = "157_fast_scm_rows.rs"]
mod t_157_fast_scm_rows;
#[path = "158_fast_scm_kotlin.rs"]
mod t_158_fast_scm_kotlin;
#[path = "159_fast_scm_judge.rs"]
mod t_159_fast_scm_judge;
#[path = "15_typegraph_d2.rs"]
mod t_15_typegraph_d2;
#[path = "160_fast_scm_ts.rs"]
mod t_160_fast_scm_ts;
#[path = "161_fast_scm_ratchet.rs"]
mod t_161_fast_scm_ratchet;
#[path = "162_graph_views_sql.rs"]
mod t_162_graph_views_sql;
#[path = "163_graph_from_ts.rs"]
mod t_163_graph_from_ts;
#[path = "164_graph_uses_ts.rs"]
mod t_164_graph_uses_ts;
#[path = "165_graph_kotlin.rs"]
mod t_165_graph_kotlin;
#[path = "166_cleave_rust.rs"]
mod t_166_cleave_rust;
#[path = "166_graph_tsi_evidence.rs"]
mod t_166_graph_tsi_evidence;
#[path = "167_graph_paths.rs"]
mod t_167_graph_paths;
#[path = "168_graph_revision.rs"]
mod t_168_graph_revision;
#[path = "169_graph_uses_rust.rs"]
mod t_169_graph_uses_rust;
#[path = "16_python.rs"]
mod t_16_python;
#[path = "170_ratchet_sites_rust.rs"]
mod t_170_ratchet_sites_rust;
#[path = "171_reach_entry.rs"]
mod t_171_reach_entry;
#[path = "172_graph_slow.rs"]
mod t_172_graph_slow;
#[path = "173_move_cross_crate.rs"]
mod t_173_move_cross_crate;
#[path = "173_rust_export_cycle.rs"]
mod t_173_rust_export_cycle;
#[path = "174_type_ladder.rs"]
mod t_174_type_ladder;
#[path = "175_cleave_ladder.rs"]
mod t_175_cleave_ladder;
#[path = "176_rename_ladder.rs"]
mod t_176_rename_ladder;
#[path = "177_crate_scope.rs"]
mod t_177_crate_scope;
#[path = "178_cli_http_parity.rs"]
mod t_178_cli_http_parity;
#[path = "178_generated_contract.rs"]
mod t_178_generated_contract;
#[path = "178_ryi_help.rs"]
mod t_178_ryi_help;
#[path = "179_codeql_baseline.rs"]
mod t_179_codeql_baseline;
#[path = "17_cfg_first_plane.rs"]
mod t_17_cfg_first_plane;
#[path = "180_call_ladder.rs"]
mod t_180_call_ladder;
#[path = "181_server_modes.rs"]
mod t_181_server_modes;
#[path = "181_ts_ladder.rs"]
mod t_181_ts_ladder;
#[path = "182_client_daemon.rs"]
mod t_182_client_daemon;
#[path = "183_fast_path_spelling.rs"]
mod t_183_fast_path_spelling;
#[path = "184_fast_recursive_receiver.rs"]
mod t_184_fast_recursive_receiver;
#[path = "184_language_feature_matrix.rs"]
mod t_184_language_feature_matrix;
#[path = "185_rust_mod_file_edges.rs"]
mod t_185_rust_mod_file_edges;
#[path = "186_quality_gate.rs"]
mod t_186_quality_gate;
#[path = "187_python_module_resolution.rs"]
mod t_187_python_module_resolution;
#[path = "188_rust_cargo_metadata.rs"]
mod t_188_rust_cargo_metadata;
#[path = "189_lift_scope_rows.rs"]
mod t_189_lift_scope_rows;
#[path = "189_bare_cli_suggestions.rs"]
mod t_189_bare_cli_suggestions;
#[path = "18_df_aux_fields_lits.rs"]
mod t_18_df_aux_fields_lits;
#[path = "190_rename_rust_slow.rs"]
mod t_190_rename_rust_slow;
#[path = "191_rust_byte_spans.rs"]
mod t_191_rust_byte_spans;
#[path = "192_typespec.rs"]
mod t_192_typespec;
#[path = "19_docs_lang_arms.rs"]
mod t_19_docs_lang_arms;
#[path = "1_move.rs"]
mod t_1_move;
#[path = "1_resolve_cli.rs"]
mod t_1_resolve_cli;
#[path = "1a_prolog_refs.rs"]
mod t_1a_prolog_refs;
#[path = "1a_resolve_raw.rs"]
mod t_1a_resolve_raw;
#[path = "1b_prolog_metacall.rs"]
mod t_1b_prolog_metacall;
#[path = "20_unresolved.rs"]
mod t_20_unresolved;
#[path = "21_kotlin_type_plane.rs"]
mod t_21_kotlin_type_plane;
#[path = "22_doc_node.rs"]
mod t_22_doc_node;
#[path = "23_df_aux_loops_nests.rs"]
mod t_23_df_aux_loops_nests;
#[path = "23_flow_cli_dispatch.rs"]
mod t_23_flow_cli_dispatch;
#[path = "24_rust_specifiers.rs"]
mod t_24_rust_specifiers;
#[path = "25_go_specifiers.rs"]
mod t_25_go_specifiers;
#[path = "25_query_digest_repo_from_path.rs"]
mod t_25_query_digest_repo_from_path;
#[path = "26_kotlin_specifiers.rs"]
mod t_26_kotlin_specifiers;
#[path = "26_parallel_dispatch.rs"]
mod t_26_parallel_dispatch;
#[path = "27_blob_cache.rs"]
mod t_27_blob_cache;
#[path = "28_package_edges.rs"]
mod t_28_package_edges;
#[path = "29_data_family.rs"]
mod t_29_data_family;
#[path = "2_df_aux_cli.rs"]
mod t_2_df_aux_cli;
#[path = "2_move_refs.rs"]
mod t_2_move_refs;
#[path = "30_rust_mod_scope_owner.rs"]
mod t_30_rust_mod_scope_owner;
#[path = "31_owned_region.rs"]
mod t_31_owned_region;
#[path = "31_tracing.rs"]
mod t_31_tracing;
#[path = "32_join_documents_once.rs"]
mod t_32_join_documents_once;
#[path = "33_v5_parity_matrix.rs"]
mod t_33_v5_parity_matrix;
#[path = "34_prolog_corpus_throughput.rs"]
mod t_34_prolog_corpus_throughput;
#[path = "35_extract_lang.rs"]
mod t_35_extract_lang;
#[path = "36_drain.rs"]
mod t_36_drain;
#[path = "37_fact_set.rs"]
mod t_37_fact_set;
#[path = "39_ts_specifiers.rs"]
mod t_39_ts_specifiers;
#[path = "3_move_rust.rs"]
mod t_3_move_rust;
#[path = "40_ts_resolve.rs"]
mod t_40_ts_resolve;
#[path = "41_move_ts.rs"]
mod t_41_move_ts;
#[path = "42_move_list.rs"]
mod t_42_move_list;
#[path = "42_ts_module_bodies.rs"]
mod t_42_ts_module_bodies;
#[path = "43_python_corpus_gaps.rs"]
mod t_43_python_corpus_gaps;
#[path = "44_go_receiver_type_params.rs"]
mod t_44_go_receiver_type_params;
#[path = "45_emit_throughput.rs"]
mod t_45_emit_throughput;
#[path = "46_resolve_scaling.rs"]
mod t_46_resolve_scaling;
#[path = "47_resolve_door_cli.rs"]
mod t_47_resolve_door_cli;
#[path = "48_kotlin_operator_calls.rs"]
mod t_48_kotlin_operator_calls;
#[path = "49_rust_resolve_scaling.rs"]
mod t_49_rust_resolve_scaling;
#[path = "4_capability_parity.rs"]
mod t_4_capability_parity;
#[path = "4_move_kotlin.rs"]
mod t_4_move_kotlin;
#[path = "4_rename_ts.rs"]
mod t_4_rename_ts;
#[path = "50_cli_crawl_defects.rs"]
mod t_50_cli_crawl_defects;
#[path = "51_go_package_resolve.rs"]
mod t_51_go_package_resolve;
#[path = "52_rust_crawl_kinks.rs"]
mod t_52_rust_crawl_kinks;
#[path = "53_ts_crawl_kinks.rs"]
mod t_53_ts_crawl_kinks;
#[path = "54_ts_module_plane.rs"]
mod t_54_ts_module_plane;
#[path = "55_diff_verb.rs"]
mod t_55_diff_verb;
#[path = "55_go_type_plane.rs"]
mod t_55_go_type_plane;
#[path = "56_scip_cli_kinks.rs"]
mod t_56_scip_cli_kinks;
#[path = "57_rust_module_plane.rs"]
mod t_57_rust_module_plane;
#[path = "58_rust_mbe.rs"]
mod t_58_rust_mbe;
#[path = "59_rust_scip_macros.rs"]
mod t_59_rust_scip_macros;
#[path = "5_move_scip.rs"]
mod t_5_move_scip;
#[path = "5_rename_rust.rs"]
mod t_5_rename_rust;
#[path = "5_scip_facts_cli.rs"]
mod t_5_scip_facts_cli;
#[path = "60_rust_corpus_scope.rs"]
mod t_60_rust_corpus_scope;
#[path = "61_own_blob.rs"]
mod t_61_own_blob;
#[path = "62_go_module_plane.rs"]
mod t_62_go_module_plane;
#[path = "63_go_inferred.rs"]
mod t_63_go_inferred;
#[path = "64_go_closure_mirror.rs"]
mod t_64_go_closure_mirror;
#[path = "65_ts_member_calls.rs"]
mod t_65_ts_member_calls;
#[path = "66_go_iface_fanout.rs"]
mod t_66_go_iface_fanout;
#[path = "67_go_multihop.rs"]
mod t_67_go_multihop;
#[path = "68_go_type_refs.rs"]
mod t_68_go_type_refs;
#[path = "68_rust_receivers.rs"]
mod t_68_rust_receivers;
#[path = "69_go_promoted.rs"]
mod t_69_go_promoted;
#[path = "69_ts_closure_mirror.rs"]
mod t_69_ts_closure_mirror;
#[path = "6_document_formats.rs"]
mod t_6_document_formats;
#[path = "6_kind_vocab.rs"]
mod t_6_kind_vocab;
#[path = "6_occurrence_text_cli.rs"]
mod t_6_occurrence_text_cli;
#[path = "70_ts_init_receivers.rs"]
mod t_70_ts_init_receivers;
#[path = "71_go_residual.rs"]
mod t_71_go_residual;
#[path = "71_rust_paths.rs"]
mod t_71_rust_paths;
#[path = "71_ts_namespace_members.rs"]
mod t_71_ts_namespace_members;
#[path = "72_go_bound_qualify.rs"]
mod t_72_go_bound_qualify;
#[path = "72_rust_traits.rs"]
mod t_72_rust_traits;
#[path = "72_ts_iface_receiver.rs"]
mod t_72_ts_iface_receiver;
#[path = "73_go_range_elem.rs"]
mod t_73_go_range_elem;
#[path = "73_ts_destructured_receiver.rs"]
mod t_73_ts_destructured_receiver;
#[path = "74_go_field_promote.rs"]
mod t_74_go_field_promote;
#[path = "74_scip_relationship_family.rs"]
mod t_74_scip_relationship_family;
#[path = "74_ts_property_arrow.rs"]
mod t_74_ts_property_arrow;
#[path = "75_rust_trait_blob.rs"]
mod t_75_rust_trait_blob;
#[path = "76_rust_variant_names.rs"]
mod t_76_rust_variant_names;
#[path = "77_rust_collapsed_span.rs"]
mod t_77_rust_collapsed_span;
#[path = "78_rust_checker.rs"]
mod t_78_rust_checker;
#[path = "79_rust_generic_args.rs"]
mod t_79_rust_generic_args;
#[path = "79_rust_qualified_type.rs"]
mod t_79_rust_qualified_type;
#[path = "79_rust_type_alias.rs"]
mod t_79_rust_type_alias;
#[path = "79_rust_type_dump.rs"]
mod t_79_rust_type_dump;
#[path = "79_rust_variant_payload.rs"]
mod t_79_rust_variant_payload;
#[path = "7_diet_deps_cli.rs"]
mod t_7_diet_deps_cli;
#[path = "7_import_ref_kind.rs"]
mod t_7_import_ref_kind;
#[path = "7_rename_kotlin.rs"]
mod t_7_rename_kotlin;
#[path = "80_py_args.rs"]
mod t_80_py_args;
#[path = "80_py_assignments.rs"]
mod t_80_py_assignments;
#[path = "80_py_decorators.rs"]
mod t_80_py_decorators;
#[path = "80_py_direct_calls.rs"]
mod t_80_py_direct_calls;
#[path = "80_py_exceptions.rs"]
mod t_80_py_exceptions;
#[path = "80_py_module_caller.rs"]
mod t_80_py_module_caller;
#[path = "80_rust_impl_owner.rs"]
mod t_80_rust_impl_owner;
#[path = "8_rename_prolog.rs"]
mod t_8_rename_prolog;
#[path = "8_scip_families_cli.rs"]
mod t_8_scip_families_cli;
#[path = "90_mutation_battery.rs"]
mod t_90_mutation_battery;
#[path = "91_origin_column.rs"]
mod t_91_origin_column;
#[path = "92_ts_checker.rs"]
mod t_92_ts_checker;
#[path = "93_rust_checker_wiring.rs"]
mod t_93_rust_checker_wiring;
#[path = "94_rust_checker_types.rs"]
mod t_94_rust_checker_types;
#[path = "95_rust_macro_callers.rs"]
mod t_95_rust_macro_callers;
#[path = "96_witness_wire.rs"]
mod t_96_witness_wire;
#[path = "97_ingest.rs"]
mod t_97_ingest;
#[path = "98_resolve_witness.rs"]
mod t_98_resolve_witness;
#[path = "99_syntax_tsi_rows.rs"]
mod t_99_syntax_tsi_rows;
#[path = "9_large_file_bounds.rs"]
mod t_9_large_file_bounds;
#[path = "9_query_cli.rs"]
mod t_9_query_cli;
#[path = "9_size_skip.rs"]
mod t_9_size_skip;
#[path = "9a_query_blob_door.rs"]
mod t_9a_query_blob_door;
#[path = "bench_normal_form.rs"]
mod t_bench_normal_form;
#[path = "golden_parity.rs"]
mod t_golden_parity;
#[path = "n_plus_one.rs"]
mod t_n_plus_one;
#[path = "ratchet_recall.rs"]
mod t_ratchet_recall;
#[path = "scip_freshness.rs"]
mod t_scip_freshness;
#[path = "scip_indexer_pick.rs"]
mod t_scip_indexer_pick;
#[path = "snapshot.rs"]
mod t_snapshot;

#[path = "193_ts_rtkq_jsx.rs"]
mod t_193_ts_rtkq_jsx;
#[path = "194_scmpp_rows.rs"]
mod t_194_scmpp_rows;
#[path = "195_scmpp_growth.rs"]
mod t_195_scmpp_growth;

#[path = "194_ts_resolve_growth.rs"]
mod t_194_ts_resolve_growth;
#[path = "195_ts_lib_globals.rs"]
mod t_195_ts_lib_globals;
#[path = "196_rust_walk_growth.rs"]
mod t_196_rust_walk_growth;

#[path = ""]
mod v5_parity {
    #[path = "199_v5_labeled_break.rs"]
    mod labeled_break;

    #[path = "200_v5_call_lines.rs"]
    mod call_lines;

    #[path = "201_v5_owners.rs"]
    mod owners;

    #[path = "202_v5_jsx.rs"]
    mod jsx;

    #[path = "203_v5_reach.rs"]
    mod reach;

    #[path = "205_deferred_jsx.rs"]
    mod deferred_jsx;
    #[path = "206_v5_capture_parity.rs"]
    pub(crate) mod capture_parity;
    #[path = "198_v5_support.rs"]
    pub mod v5_support;

    #[cfg(feature = "cli")]
    #[path = "212_ts_df_owner_flags.rs"]
    mod df_owner_flags;

    #[path = "210_ts_df_lift_owners.rs"]
    mod df_lift_owners;
    #[cfg(feature = "cli")]
    #[path = "211_hooks_query.rs"]
    mod hooks_query;
}

#[test]
fn every_test_file_is_a_module_here() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let listed = include_str!("all.rs");
    let missing: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".rs") && n != "all.rs")
        .filter(|n| !listed.contains(&format!("#[path = \"{n}\"]")))
        .collect();
    assert!(missing.is_empty(), "add to tests/all.rs: {missing:?}");
}

#[path = "support/1_df_increment.rs"]
mod df_increment_support;
#[path = "211_rust_names.rs"]
mod t_211_rust_names;

#[path = "support/2_fixture_runner.rs"]
mod fixture_runner;
#[path = "support/3_v5_normalize.rs"]
mod v5_normalize;
#[path = "support/4_golden_parity.rs"]
mod golden_parity_support;

#[path = "support/5_scip_families.rs"]

mod scip_families_support;

#[cfg(feature = "cli")]
#[path = "support/6_sqlite.rs"]
mod sqlite_support;

#[path = "support/8_cli_crawl.rs"]
mod cli_crawl_support;

#[path = "support/9_scip_freshness.rs"]
mod freshness_support;

#[path = "support/10_tsi_rows.rs"]
mod tsi_rows_support;

#[path = "support/11_cfg.rs"]
mod cfg_support;

#[cfg(feature = "cli")]
#[path = "support/12_http.rs"]
mod http_support;

#[path = "support/13_ingest.rs"]
mod ingest_support;

#[path = "support/14_scip_facts.rs"]
mod scip_facts_support;

#[path = "../bench/6_wall_contracts.rs"]
mod wall_bench;

#[cfg(feature = "rust-checker")]
#[path = "support/15_rust_walk.rs"]
mod rust_walk_support;

#[path = "../bench/7_ts_module_scaling.rs"]
mod t_ts_module_scaling;

#[path = "support/16_rust_call_contract.rs"]
mod rust_call_contract_support;

#[path = "support/17_go_residual.rs"]
mod go_residual_support;

#[path = "support/18_rust_module_plane.rs"]
mod rust_module_plane_support;

#[path = "support/19_python_call_grind.rs"]
mod python_call_grind_support;
