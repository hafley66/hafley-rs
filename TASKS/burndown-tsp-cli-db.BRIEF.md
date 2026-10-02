# boop CLI, DDL, migrations from TypeSpec (branch burndown/tsp-cli-db, base = burndown/hermetic 2d666ded)

Base includes the hermetic test guard. Read crates/boop-store/src/_0_test_paths.rs first.

## Today
| piece | source of truth |
| --- | --- |
| ryi CLI (the pattern) | crates/sprefa-extract/schema/cli/{ops,domain}.tsp + tspconfig.yaml -> `@hafley66/alloy-rs` (link: /Users/chrishafley/projects/hafley-tsp/packages/rust) -> crates/ryi-proto/src/gen/cli_auto.rs |
| boop CLI | 31 hand-written clap derives in crates/boop*/src |
| /Users/chrishafley/projects/boop2/schema/5_cli.tsp | 172 decls traced from those derives; no clap decorators. READ ONLY, do not edit boop2 |
| /Users/chrishafley/projects/boop2/fixtures/help | 156 `boop <path> --help` captures |
| boop DDL | 110 `CREATE TABLE` literals in crates/boop-store/src/*.rs; crates/boop-store/sql/40_schema.sql |
| /Users/chrishafley/projects/boop2/schema/2_tables.tsp | 69 table models, stale at user_version 38. READ ONLY |
| migrations | Rust code (55 `user_version` sites) + crates/boop-store/sql/{39,40}_*.sql |

## Slice 1: CLI from tsp (commit 1)
- Create crates/boop/schema/cli/{package.json,tspconfig.yaml,ops.tsp} copying the ryi layout and emitter.
- Seed ops.tsp from boop2 5_cli.tsp; add the clap decorators ryi uses (@positional, @valueName, @requires,
  @conflictsWith, @valueDelimiter, @encodedName, ...).
- Emit to crates/boop/src/gen/cli_auto.rs (or the crate that owns the derives). Replace the 31 derives with the
  generated types. Handlers keep their bodies.
- Gate: `boop <path> --help` byte-equal to all 156 boop2 fixtures/help files (write a bats or shell check, not Python).
- Generated files are never hand-patched. Emitter gaps: list them in REPORT (file:line in hafley-tsp packages/rust,
  minimal tsp repro). Do not edit hafley-tsp.

## Slice 2: DDL from tsp (commit 2)
- crates/boop-store/schema/tables.tsp: port 2_tables.tsp, refresh to schema 40 (enum columns from 40_closed_sets.sql;
  CHECK only on closed Rust sets, audit: plans/dict-closed-sets/18_check_audit.tsv).
- Emit with /Users/chrishafley/projects/hafley-tsp/packages/sql to crates/boop-store/sql/schema.sql.
- Store creates a fresh db from schema.sql; delete the CREATE TABLE literals it replaces.
- Gate: fresh Store::open in a sandbox; `sqlite3 db .schema` normalized == schema.sql.

## Slice 3: migrations (commit 3)
- v40 is the floor (user 2026-10-02). Delete the <40 upgrade code. Opening a db with user_version < 40 errors with
  a message naming the version.
- Baseline: v40 schema.sql = migration 40. Runner: rusqlite_migration (or refinery) with migrations as
  include_str! of crates/boop-store/migrations/NN_name.sql.
- Diff tool: Atlas CLI (`atlas migrate diff` against a dev sqlite in memory). If `atlas` is missing:
  `brew install ariga/tap/atlas`. Add a just/cargo-xtask-free shell script crates/boop-store/scripts/migrate-diff.sh:
  emit tsp -> atlas diff old schema.sql vs new -> migrations/NN.sql. Data steps are hand-added into that file.
- Gate: copy of /Users/chrishafley/backups/boop/boop-v40-20261001-210341.db (4.5 GB, copy into scratch, delete after)
  opens with the new store; `.schema` equals a fresh db's; `SELECT count(*)` of favorites (306) and tags (418)
  unchanged. Prove the diff tool round-trip with one demo migration (add a nullable column on a scratch copy
  of tables.tsp), then drop the demo, do not commit it.

## Rules
- Tests and every boop binary run with `HOME=<worktree>/scratch/home`, BOOP_DB / BOOP_MAIL_DIR unset. Never real HOME.
- Gates each commit: `cargo test -p boop-store -p boop-proc -p boop-harness --lib`, `cargo test -p boop --bin boop`,
  contract suite from /Users/chrishafley/projects/boop2-harmonize: `BOOP_BIN=<built boop> bash tests/run.sh` 0 not-ok.
  Do not run crates/boop/tests/{worktree_reclaim_e2e,tui_revive_e2e,omp_live_trait_e2e}.rs (env-sensitive, pre-existing).
- Rust: workspace already sets [profile.dev] opt-level=1 and deps opt-level=3; keep it.
- Numbered Rust files: `_N_name.rs` with plain `mod _N_name;`, never #[path]. No Python. CARGO_BUILD_JOBS=4.
- Keep field names/casing identical across tsp, SQL, Rust (no serde renames).
- Commit each slice before moving on; messages end `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`.
- Do not push. Do not install boop. REPORT.md at worktree root: per slice commit sha, gate output lines, emitter gaps.
