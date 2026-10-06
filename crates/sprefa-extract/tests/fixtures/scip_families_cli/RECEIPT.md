# SCIP family folding receipt

22 original tests passed on the current sources with `cli,ts-checker` before deletion. The fixture command sequences preserve the same arguments, copied source bytes, file edits, timestamp values, supplied indexes, PATH plants and process-group timeout script.

Every case records exit status, complete JSONL records and stderr. Original byte-exact goldens, resolve equivalence and cache-reuse equality remain active in shared runner comparisons. The timeout case retains its 20-second guard and dead-grandchild check. Supplied-index cases retain sentinel checks. The roster records every language, binary, marker and install hint.

Initial capture comparison produced 20 changed stderr cells from tracing timestamps, process IDs, temporary indexer paths and durations. All fact records, statuses and timeout receipts matched. Fixture commands disable tracing through `RUST_LOG=off`, `DL_TRACE=0`, `DL_TRAIL=0`. Comparison with that controlled capture again changed only those 20 stderr cells. Existing golden files are unchanged. Temporary workspace paths are normalized to `$work`; unpinned index mtimes/staleness and Rust's unpinned tool version are excluded per manifest. Explicit timestamp-evidence cases retain both fields.

Deleted function -> covering lines in `output.snap`:

- `only_real_scip_resolves_the_cross_file_call_the_heuristic_cannot` -> 5-2365.
- `the_discrimination_holds_through_rust_analyzer_too` -> 2366-3189.
- `the_scip_family_stream_is_the_v5_relation_vocabulary` -> 3190-3302.
- `the_diet_scip_family_stream_is_the_fast_output` -> 3303-5430.
- `diet_scip_is_the_resolve_pass_with_both_arms_plus_the_scm_rows` -> 5431-5931.
- `the_rust_plane_produces_the_relations_only_a_real_index_carries` -> 5932-6638.
- `an_explicit_family_index_is_read_directly_without_spawning_an_indexer` -> 6639-6751.
- `missing_and_invalid_explicit_family_indexes_fail_without_rebuilding` -> 6752-6766.
- `explicit_index_requires_project_root_outside_the_scip_family` -> 6767-6773.
- `explicit_index_conflicts_with_indexer_selection_and_build` -> 6774-6780.
- `explicit_scip_index_reports_source_timestamp_evidence` -> 6781-7112.
- `stale_cached_index_without_an_indexer_emits_only_a_skip` -> 7113-7127.
- `family_rebuilds_for_content_edits_and_new_or_deleted_sources` -> 7128-7326.
- `an_existing_index_is_reused_and_yields_identical_rows` -> 7327-7547.
- `a_root_with_no_installed_indexer_emits_a_named_skip_and_exits_zero` -> 7548-7562.
- `a_root_with_no_marker_file_says_so_rather_than_streaming_nothing` -> 7563-7577.
- `an_indexer_past_its_budget_is_killed_with_its_whole_process_group` -> 7578-7596.
- `the_scip_family_takes_exactly_one_root` -> 7597-7603.
- `an_unknown_mask_family_is_a_named_error` -> 7604-7610.
- `the_scip_family_never_reuses_the_passthrough_occurrence_tag` -> 7611-7834.
- `the_roster_carries_v5s_six_languages` -> 7835-7898.
- `the_three_added_languages_detect_and_skip_by_name` -> 7899-7940.

Gates: folded concern 1 passed; `t_186_quality_gate` 4 passed; module inventory 1 passed. No production-source edits, installs, merge or push.
