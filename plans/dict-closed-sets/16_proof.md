# dict-closed-sets, schema 40

The migration replaces 22 integer dictionary references across 17 tables with text columns and drops 14 dictionary tables. Eighteen columns retain finite membership CHECKs. Four columns accept open text: raw tool name, transcript role, price source, and legacy root-stamp harness. Open dictionary ids and complete contents remain equal. Producer citations and closure decisions are in `18_check_audit.tsv` and `19_audit_notes.md`.

## Preservation proof

`0_rehearse.sh` copies `/Users/chrishafley/backups/boop/boop-20261001-162007.db` into this lane's `scratch/`, runs schema 38 → 39 → 40, and deletes its database copies on exit. The schema 39 stage uses the explicit scratch-only ignored store test. Schema 40 uses `examples/3_migrate.rs`.

Required executable inputs: `BOOP_BEFORE_BIN` (a saved binary supporting the integer dictionary schema), `BOOP_BIN` (the changed binary), `STORE_TEST_BIN` (the store unit-test executable), and `MIGRATE_BIN` (the migration example). Run from the lane with Bash; SQLite performs the queries. No Python is used.

- `1_columns.tsv`: all 22 reference mappings.
- `2_versions.tsv`: observed schema versions at each stage.
- `3_counts_{38,39,40}.tsv`: every affected table row count.
- `4_values_{38,39,40}.tsv`: independently grouped decoded values and multiplicities, including NULL.
- `5_open_dicts.tsv`: all 17 open dictionaries have equal complete row contents and ids.
- `8_checks.tsv`: preservation comparisons and SQLite integrity check pass.
- `1_tallies.sh`: derives column, table, and dictionary summaries using Bash and SQLite from the committed raw TSVs.
- `12_column_tallies.tsv`, `13_table_tallies.tsv`, `14_dictionary_tallies.tsv`: detailed tallies.
- `15_tallies.html`: browser grid with per-column filters, including the CHECK audit.

The referenced value counts for trace classification, delivery, kind, and verb are 12, 8, 12, and 10 respectively. Relation kind has zero referenced values and retains the five declared enum variants. CHECK domains also include declared producer values absent from the backup, such as the mood attribute key.

## Timings

`7_timings.tsv` records three wall-clock CLI repetitions per query on isolated copies. Search is `boop db search boop --limit 20 --format text`. The turn query reads 100 ordered turns from one session, decoding the old role join before migration and reading text directly afterwards. CLI logging uses separate timing copies, preserving the migration proof database.

The rerun baseline uses the existing installed `boop 0.0.10 (248dfdd3-dirty)` on a schema 38 copy; the changed binary uses a schema 40 copy. This older baseline includes other code differences, so the search difference cannot be attributed to the enum migration alone. The original baseline binary had been deleted as requested; its earlier timing measurements are preserved in `20_original_timings.tsv`. Rerun medians, in milliseconds:

| Query | Before | After |
| --- | ---: | ---: |
| Search | 20562.214 | 797.798 |
| Agent-turn read | 16.384 | 18.061 |

## Gates and cleanup

The required Rust binary test command passes 170 tests. The complete store suite passes 246 tests, with one ignored rehearsal driver executed separately in the preservation proof. Key-type conflicts are zero and the TypeSpec build passes. The contract suite passes 71 cases, skips four, and reports zero not-ok. Contract results are in `9_contract.log` and `9_contract.exit`; the runner is unchanged and uses the real store read-only favorite/tag tripwire. The coordinator authorized this read-only tripwire after the earlier scratch override returned exit 98. Scratch databases, helper scripts, and the saved baseline binary were deleted after rehearsal.
