# boop CLI, DDL, migrations from TypeSpec

## Today

| piece | source of truth | generated? |
| --- | --- | --- |
| ryi CLI | crates/sprefa-extract/schema/cli/{ops,domain}.tsp -> `@hafley66/alloy-rs` -> ryi-proto/src/gen/cli_auto.rs | yes |
| boop CLI | 31 clap derives in crates/boop*/src (3 files) | no |
| boop2/schema/5_cli.tsp | 172 decls traced from the derives; 2 decorators (`@effort`), no clap decorators | descriptive |
| boop2 fixtures/help | 156 `boop <path> --help` captures | gate input |
| boop DDL | 110 `CREATE TABLE` literals in crates/boop-store/src/*.rs | no |
| boop2/schema/2_tables.tsp | 69 models, says user_version 38 | descriptive, stale |
| migrations | Rust code, 55 `user_version` sites + crates/boop-store/sql/{39,40}_*.sql | no |

ryi ops.tsp decorators the boop port needs: `@valueName` 49, `@requires` 17,
`@conflictsWith` 14, `@positional` 12, `@valueDelimiter` 4, `@encodedName` 3.

## Slice 1: CLI

```
crates/boop/schema/cli/ops.tsp ─tsp compile─▶ @hafley66/alloy-rs ─▶ crates/boop/src/gen/cli_auto.rs
crates/boop/src/cli/*.rs handlers match on generated enums
```

1. Copy ryi layout: `crates/boop/schema/cli/{package.json,tspconfig.yaml,ops.tsp}`.
2. Seed ops.tsp from boop2/schema/5_cli.tsp; add clap decorators per field.
3. Emit; replace the 31 derives with generated types; handlers unchanged.
4. Gate: 156 help fixtures byte-equal; boop2 contract suite (`BOOP_BIN=... bash tests/run.sh`) zero not-ok; sandboxed HOME.
5. Emitter gaps found -> hafley-tsp packages/rust issues, not hand patches in gen/.

## Slice 2: DDL

```
crates/boop-store/schema/tables.tsp ─▶ hafley-tsp packages/sql ─▶ crates/boop-store/sql/schema.sql
```

1. Move 2_tables.tsp into hafley-rs, refresh to v40 (enum columns from 40_closed_sets).
2. Gate: fresh `Store::open` in sandbox; `sqlite3 .schema` normalized == emitted schema.sql.
3. Store creates fresh dbs from schema.sql; CREATE literals deleted.

## Slice 3: migrations

```
schema.sql@prev + schema.sql@new ─atlas migrate diff─▶ crates/boop-store/migrations/NN_name.sql
migrations/*.sql ─include_str!─▶ rusqlite_migration (tracks user_version)
```

1. Baseline = v40 schema.sql as migration 40.
2. Each schema change: edit tsp -> emit -> `atlas migrate diff` -> hand-add data steps in the generated file -> commit both.
3. Gate: fresh db and copy of a v40 backup both reach the same `.schema`; favorites/tags counts preserved.

## Open

- Keep the 38->39->40 Rust upgrade path, or require v40 (live db is v40; backups v38 exist).
- Slice order: 1 is independent of 2/3.

## Tool dry-run 2026-10-02 (scratch, 40_schema.sql, 60 tables, 9 views, 2 triggers)

| tool | result |
| --- | --- |
| sqlite3def 3.11.26 | parse error on column `sequence` (agent_delivery_transition), then `query` (agent_fetch) |
| atlas v1.3.4, raw schema | views need `atlas login`; 24 DROP/CREATE pairs for inline UNIQUE even on no-op; DROP fails on apply |
| atlas, UNIQUE hoisted to named indexes, views/triggers stripped | no-op: synced. change (add column, NOT NULL DEFAULT rebuild w/ IFNULL, CHECK grow, index): applied, re-diff synced |

Emitter rule: no inline UNIQUE; `CREATE UNIQUE INDEX <table>_<col>`. Views/triggers recreated per migration outside atlas.
