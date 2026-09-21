# Lane: `ryi cleave SRC#ITEM DEST`, TypeScript arm

Repo `~/projects/hafley-rs`, crate `crates/sprefa-extract` (binary `ryi`).
This crate is its OWN workspace root. Every cargo command runs from
`crates/sprefa-extract` inside your worktree. Never `cd` to another checkout.

Read first, in order:
1. `crates/sprefa-extract/plans/2026-09-20-graph-views-and-cleave.md`, section
   `## L4 cleave` (the step trace is the spec)
2. `issues/item-move-import-closure/item.md`
3. `crates/sprefa-extract/AGENTS.md`
4. `src/0_move.rs`, `src/move_cx.rs`, `src/0_rename.rs`, `src/lang/ts_rename.rs`

## What ships

```
ryi cleave SRC#ITEM DEST [--root DIR] [--state DIR] [--drag] [--commit]
                         [--verify CMD] [--text-refs] [--json]
```

One item leaves SRC and lands in DEST (created if missing). Its free names are
partitioned against SRC's `specifier` rows: imported-into-SRC and package
specifiers travel to DEST (deduped against DEST's own imports); SRC loses the
specifiers nothing left in SRC references; every importer of `SRC#ITEM` is
respelled to DEST, splitting the specifier when other names stay in SRC.
Dry run is the default, exactly as `0_move.rs:57`. `--drag` pulls same-file
private helpers the item references, to a fixpoint whose iteration count is a
`cleave_plan` field.

TypeScript only. Out of scope, stated in the help text as such: Rust,
cross-language, a type with its `impl` blocks, an item whose free names carry
a `-` grade (print the names and the `ryi graph --uses` command, exit 0).

## COMMIT CONTRACT

One commit per phase. Every commit carries, one `-m` per line after the subject:

```
Boop-Status: wip
Boop-Check: <exact command> -> <exact counted result>
Boop-Trace: <trace file> -> <first span over 1s, or "none">
Refs-Issue: @item-move-import-closure
```

Never `rc=0`; counted results. Last commit `Boop-Status: done`. Stuck:
`Boop-Status: blocked` plus one `Boop-Ask`.

## TRACE LAW

Every command you launch runs under `timeout 10` (cargo builds and the full
gate under `timeout 600`). A hit 10s timeout is a defect, reported as such.
Every `ryi` run carries `HAFLEY_TRACE=<path under std::env::temp_dir()>`. The
new test sets it the same way (pattern `tests/150_fast_scm_kotlin.rs:63`).
Spans are chrome `B`/`E` pairs.

## Owned files

- NEW `src/0_cleave.rs` (clap `CleaveCli`, plan, apply)
- NEW `src/cleave_cx.rs` only if `0_cleave.rs` passes 600 lines
- `src/bin/ryi.rs`: the `#[path]` mod line and one dispatch arm next to
  `move`/`rename`
- `src/types.rs`: `CleavePlan`, `CleaveSpecifier`, `CleaveDrag` rows next to
  the move plan types; `schema/0_wire_types.tsp` + regenerated output
- NEW `tests/155_cleave_ts.rs`, NEW `tests/fixtures/cleave_ts/` (the three
  files from the plan's step trace, plus the drag variant and a two-level
  drag variant)

FORBIDDEN: `src/0_move.rs`, `src/0_rename.rs`, `src/lang/ts_rename.rs`,
`src/lang/rust_rename.rs`, every other `lang/*.rs`, `docs/`. Reuse `MoveCx`,
`Respell`, `replace_action`, `stage_and_commit`, `VerifyJournal` as they are.
If one of them needs a new `pub`, add the `pub` and nothing else, and name it
in the commit body.

## Phases

| phase | content | check |
| --- | --- | --- |
| 1 | rows + tsp + clap skeleton; `ryi cleave --help` prints the surface | `cargo build --features cli` last line |
| 2 | plan: resolve once, item span, free names, partition, orphan set, DEST dedupe, caller set; `--json` prints the plan, tree untouched | fixture: plan lists 3 travelling specifiers, 3 orphans, 1 caller; digest of every fixture file unchanged |
| 3 | apply under `--commit` through soopy stages; `--verify` rollback | after apply: `util.ts` lost 3 import lines, `config.ts` gained 2, `app.ts` gained 1, `grep -c node:path config.ts` = 1; verify-fail restores all three |
| 4 | `--drag` fixpoint | private-helper fixture: 1 drag candidate with `--drag`, 0 without; two-level fixture: iteration count 2 |
| 5 | gate | `timeout 600 cargo test --features cli --no-fail-fast` counted; baseline 191 binaries, 1006 passed |

## Style laws

Match the surrounding file. No em dashes. No `honest`, `load-bearing`,
`substrate`, `provenance`, `regime`. Comments state facts. Tests through the
binary, no mocks. Numeric file prefixes as the crate does.
