# Golden parity folding receipt

13 original tests passed on the current sources with `cli,ts-checker` before deletion. The fixture evaluator passed its reference capture while retaining the oracle comparisons and ratchet assertions. The new output snapshot came from that successful capture. Existing v5 JSONL oracles and `tests/RATCHET.tsv` are unchanged.

The seven manifests drive the same 13 program fixtures, four indexer corpora, follow-up-card status sequence, and missing-origin histogram. Per-case paths, extension filters, local-symbol policy, exclusions and follow-up data live in the manifests. The shared evaluator keeps floor/ceiling checks and the direct oracle comparisons. Informational migration metrics retain their unasserted status through `report_ledger`, invoked for each corpus case.

Deleted function -> covering lines in `output.snap`:

- `ported_facets_match_v5` -> 7-52,56-69,81-126,130-249,252-280,291-447,451-519,531-601,616-686,694-727,735-813,823-943,947-1045.
- `type_edge_resolve_parity_ts` -> 728-732,814-820,944-944,1046-1057.
- `type_edge_resolve_parity_go` -> 53-53,70-78,127-127.
- `type_edge_resolve_parity_python` -> 281-288,448-448,520-528.
- `type_edge_resolve_parity_rust` -> 609-613,687-691.
- `deferred_and_v6_only_ledger` -> 5-1059 (fixture corpus; informational diagnostics remain in report_ledger).
- `call_resolve_scip_ratchet_ts` -> 1060-2076.
- `call_resolve_scip_ratchet_go` -> 2077-2288.
- `call_resolve_scip_ratchet_rust` -> 2289-2765.
- `call_resolve_scip_ratchet_kotlin` -> 2766-2968.
- `ratchet_bump_requires_a_followup_card_for_each_wrong_target_class` -> 2969-2986.
- `ratchet_pin_charges_an_origin_the_run_dropped` -> 2987-2990.
- `rust_doc_parity` -> 602-608.

Gate results: folded concern 1 passed; `t_186_quality_gate` 4 passed; module inventory 1 passed. The concern gate took 5.22 seconds; the quality gate took 31.79 seconds. No production-source edits, installs, merge or push.
