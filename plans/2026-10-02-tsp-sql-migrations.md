# All boop SQL + migrations authored in TypeSpec

## Inventory (2026-10-02)

| where | what | count |
| --- | --- | --- |
| hafley-rs crates/boop-store/src/*.rs | `CREATE TABLE` string literals | 110 |
| hafley-rs crates/boop-store/src/*.rs | `user_version` references | 55 |
| hafley-rs crates/boop-store/sql/ | .sql files (39 intern fixture, 40 closed sets, 40 schema, 2 lane fixtures) | 5 |
| boop2 schema/2_tables.tsp | table models, descriptive only, doc says user_version 38 | 69 models |
| hafley-tsp packages/sql | DDL emitter (facts, intern, dialect, emit_sql) | no migration code |
| hafley-tsp packages/rusqlite, sqlx | Rust storage writers from tsp | 4/4 done per TASKS/2_rusqlite_STATUS.md |

## Migration tool candidates

| tool | input | output | SQLite | data migrations |
| --- | --- | --- | --- | --- |
| Atlas (`atlas migrate diff`) | desired schema .sql/.hcl + dev db | versioned .sql files, checksummed `atlas.sum` | yes | hand-edit generated file |
| sqldef `sqlite3def` | desired schema .sql | applies diff directly, `--dry-run` prints DDL | yes | none |
| rusqlite_migration | hand-written ordered SQL | runs them, tracks `user_version` | yes | yes, by hand |
| refinery | hand-written V1__x.sql files | runs them, history table | yes | yes, by hand |

## Pipeline

```
schema/*.tsp ──tsp compile (hafley-tsp sql emitter)──▶ desired.sql
desired.sql + previous desired.sql ──atlas migrate diff──▶ migrations/NN_name.sql
migrations/*.sql ──include_str! (rusqlite_migration)──▶ boop-store open()
```

- 38→40 carried data transforms (`39_interned_fixture.sql` INSERT…SELECT into dict_*). A diff tool emits DDL only; data steps stay as authored SQL in the generated migration file or a tsp decorator carrying the SQL text.
- Queries (110 literals include views/indexes) are a separate slice from DDL.

## Store path resolution (crates/boop-store/src/ident.rs:1518)

1. test root (thread-local, test builds)
2. `BOOP_DB`
3. `BOOP_MAIL_DIR/boop.db`
4. `~/.agent/boop.db`

Non-test sites naming `BOOP_DB` or `".agent"`: 21. An ancestor walk from cwd (e.g. `.boop/boop.db`) inserts before step 4. The store indexes every harness transcript machine-wide; a per-directory db holds only what that directory's commands write.
