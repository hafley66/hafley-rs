# dict-closed-sets, schema 40

The migration replaces 22 integer dictionary references across 17 tables with checked text columns and drops 14 closed dictionary tables. Open dictionary ids and complete contents remain equal. All listed closed dictionaries are retained as enum domains; none required an open vocabulary exception.

## Preservation proof

`0_rehearse.sh` copies `/Users/chrishafley/backups/boop/boop-20261001-162007.db` into this lane's `scratch/`, runs schema 38 → 39 → 40, and deletes its database copies on exit. The schema 39 stage uses the explicit scratch-only ignored store test. Schema 40 uses `examples/3_migrate.rs`.

Required executable inputs: `BOOP_BEFORE_BIN` (the pre-change schema 39 binary), `BOOP_BIN` (the changed binary), `STORE_TEST_BIN` (the store unit-test executable), and `MIGRATE_BIN` (the migration example). Run from the lane with Bash; SQLite performs the queries. No Python is used.

- `1_columns.tsv`: all 22 reference mappings.
- `2_versions.tsv`: observed schema versions at each stage.
- `3_counts_{38,39,40}.tsv`: every affected table row count.
- `4_values_{38,39,40}.tsv`: independently grouped decoded values and multiplicities, including NULL.
- `5_open_dicts.tsv`: all 17 open dictionaries have equal complete row contents and ids.
- `8_checks.tsv`: preservation comparisons and SQLite integrity check pass.
- `1_tallies.sh`: derives column, table, and dictionary summaries using Bash and SQLite from the committed raw TSVs.
- `12_column_tallies.tsv`, `13_table_tallies.tsv`, `14_dictionary_tallies.tsv`: detailed tallies.
- `15_tallies.html`: browser grid with per-column filters.

The referenced value counts for trace classification, delivery, kind, and verb are 12, 8, 12, and 10 respectively. Relation kind has zero referenced values and retains the five declared enum variants. CHECK domains also include declared producer values absent from the backup, such as the mood attribute key.

## Timings

`7_timings.tsv` records three wall-clock CLI repetitions per query on isolated copies. Search is `boop db search boop --limit 20 --format text`. The turn query reads 100 ordered turns from one session, decoding the old role join before migration and reading text directly afterwards. CLI logging uses separate timing copies, preserving the migration proof database.

The baseline copy starts at schema 38 and the pre-change binary migrates it to 39 on first access. The changed copy starts at schema 40. Medians, in milliseconds:

| Query | Before | After |
| --- | ---: | ---: |
| Search | 884.091 | 777.006 |
| Agent-turn read | 15.502 | 15.284 |

## Gates and cleanup

The required Rust binary test command passes 170 tests. The complete store suite passes 245 tests, with one ignored rehearsal driver executed separately in the preservation proof. Key-type conflicts are zero and the TypeSpec build passes. Contract results are in `9_contract.log` and `9_contract.exit`; the runner is unchanged and uses the real store read-only favorite/tag tripwire. The coordinator authorized this read-only tripwire after the earlier scratch override returned exit 98. Scratch databases, helper scripts, and the saved baseline binary were deleted after rehearsal.
