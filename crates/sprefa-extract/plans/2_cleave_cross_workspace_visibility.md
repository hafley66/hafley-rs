# Rust cleave: visibility failure across independent Cargo workspaces

Status: fixed in isolated worktree 2026-10-14; focused regressions pass; crate gate completed with three remaining unrelated failures detailed below.

Observed command, from `/Users/chrishafley/projects/hafley-games`:

```sh
ryi cleave 'crates/drill/src/_8_combat.rs#AttackId' crates/drill/src/_1a_moves.rs --root . --json
```

Diagnostic:

```text
rust-analyzer cannot establish visibility of crates/drill/src/_1a_moves.rs#AttackId from driller/src/_5_render.rs
```

The enum was still in its source file during this dry run. The proposed destination
existed after successful, separately planned/applied cleaves of `MoveProfile` and
`HitboxData`. The latter also moved its implementation. An earlier enum attempt
reported `ungraded HitboxData`; after moving that dependency, the visibility
failure above occurred. Both the simulation package and its host have independent
Cargo workspace roots. The host consumes the simulation through a path dependency.
The dry run made no enum edits. The observed shell sequence returned zero despite
the refusal; investigate whether that reflects the tool's intended exit contract.

This is reproduction context, not an established root cause. The game cleanup has
since moved the enum and implementations manually while preserving public re-exports.
Do not mutate that active checkout to reconstruct the failure.

Required investigation and fix:

- Build a minimal, independently named fixture with a library and path-dependent
  consumer in separate Cargo workspaces; move a public enum and its implementations
  into a new/existing module. Include the preceding dependency move if necessary.
- Determine whether proposed-state visibility, workspace selection, source freshness,
  or export/import repair causes the refusal. Preserve actual visibility restrictions.
- Repair the general Rust refactoring path. No game-specific names, paths, enum
  variants, Cargo manifests or diagnostic exceptions in production logic.
- Verify public consumers and re-exports, and rejection of genuinely inaccessible
  destinations. Include dry-run non-mutation and applied Cargo-check evidence.
- Run the crate's relevant tests and `cargo test --features cli`; distinguish any
  existing failures. Follow crate AGENTS.md and the hafley-rs repository skill.

Coordinator scope: the primary agent continues cleanup in hafley-games. This worker
owns only the sprefa-extract fix, regression fixtures and this issue record. Leave
unrelated working-tree changes untouched; do not publish or push.

## Investigation evidence

- Installed `ryi` and `ryii`: commit `27aa26bb57ef665dac57a4ac60b7f6f1d8ec12e9`,
  built 2026-10-11 from `/Users/chrishafley/projects/hafley-rs-wt/ryi-reconcile`.
- The original requested checkout `/Users/chrishafley/projects/hafley-rs` is on
  `claude-375-py-cycle` at `811fa6a7`, which predates this visibility subsystem.
  No obsolete subsystem was copied into that checkout.
- Patch worktree: `/Users/chrishafley/projects/hafley-rs-wt/cleave-workspace-visibility`,
  branch `fix/cleave-workspace-visibility`, based on `4c1fb405` (installed source
  plus unrelated boop changes). No game source changes.
- Minimal fixture: independent `signal-domain` library and `signal-client` consumer
  workspaces, normal Cargo path dependency, public `Signal` enum with an impl,
  public crate-root re-export, and a preceding `Level` plus impl move.
- Installed `ryii` reproduces exactly:
  `rust-analyzer cannot establish visibility of library/src/_1_destination.rs#Signal from consumer/src/lib.rs`.
  Direct `ryii` exits **2**. The original observed zero status was not reproduced
  by invoking the worker binary directly.
- Root cause: `discover` used Cargo `--no-deps`, and `names_project` emitted only
  workspace members and discarded edges to nonmembers. The consumer's Names
  host therefore had no module for the library destination, even after syncing
  the complete proposed text overlay. `can_name` returned `None` for that missing
  target module. The item being absent from the on-disk destination during a
  dry run is expected; the planner already overlays the proposed state.
- Fix: follow explicit local Cargo dependency paths using offline `--no-deps`
  metadata, retain only each requested dependency package, add dependency library
  targets/edges to the Names graph, and include local dependency/workspace
  manifests in the cached content identity. Registry sources/sysroot remain
  outside this Names load. Visibility decisions continue through rust-analyzer.
- Historical plan located and read at
  `/Users/chrishafley/projects/sprefa/v6/plans/2026-07-24-extract-go-closeout-and-resolve4.md`.
  Its old worktree and increment lists are historical; current authorized scope
  is this bug fix. No dependencies or captured oracle snapshots changed.

## Validation

- New regression `t_166_cleave_rust::separate_workspaces_preserve_visibility_and_consumers`:
  passed, 1 test, 1.80 seconds. Covers ordinary and renamed path dependencies,
  existing/new destination modules, preceding dependency and impl move, enum and
  impl move, dry-run nonmutation, public re-export preservation, private-ancestor
  rejection with nonmutating apply, and consumer `cargo check --offline`.
- Existing `t_166_cleave_rust::whole_output`: passed.
- Independent applied probes for ordinary and renamed dependencies both passed
  consumer Cargo checks. Scratch: `/private/tmp/ryi-cleave-probe-vb3lnf0a`.
- Full `cargo test --features cli --no-fail-fast -- --test-threads=2` completed:
  initial integration result 680 passed, 21 failed, 29 ignored; library 2 passed,
  5 failed; binary tests 17 passed; Rust path-declaration target 1 passed.
  One Cargo job; isolated target `/private/tmp/ryi-cleave-visibility-target`.
  Log: `/private/tmp/ryi-cleave-gate.log`.
- The isolated checkout lacked the installed source's ignored TypeScript runtime
  and the separately built client artifact. Linked
  `ts7/node_modules` to the existing ryi-reconcile runtime and copied the installed
  `ryi` client into this isolated target only. No npm/network installation and no
  change to installed binaries. These runtime prerequisites are not in the patch.
- Reran only failed targets after prerequisite repair: library **7/7 passed**;
  integration **18 passed, 3 failed**. Combined integration evidence:
  **698 passed, 3 failed, 29 ignored**. The client/daemon fixture passed.
  Logs: `/private/tmp/ryi-cleave-lib-retry.log`, `/private/tmp/ryi-cleave-gate-retry.log`.
- Remaining gate failures, not changed or snapshot-accepted:
  - `t_172_graph_slow::whole_output`: snapshot says `1 edges` / `0 edges`;
    unchanged CLI emits `1 nodes` / `0 nodes`. Graph rows otherwise match.
  - `t_8_scip_families_cli::whole_output`: fixture expects Rust SCIP files to
    own modules; current fast reader emits three `not_in_module_tree` rows.
    Installed pre-fix worker and patched worker produced byte-identical fast
    stdout for this exact Rust fixture. Baseline receipts:
    `/private/tmp/ryi-gate-baseline-4q08ed6p/rust_scip_fast.{installed,patched}.stdout`.
  - `t_98_resolve_witness::whole_output`: expects a `(semantic, tsgo)` run,
    but the required `cli`-only gate does not enable `ts-checker`;
    `ts_checker::answer` returns `NotBuilt` under that feature combination.
- The skill's historical two root-prefix oracle failures did not describe this
  source version: golden parity passed after runtime setup. No baseline-green
  claim is made for the full gate.
- `git diff --check`: passed. No dependency manifest/lock changes or snapshot
  regeneration. Installed binaries have not been replaced.
- Baseline reproduction scratch: `/private/tmp/ryi-cleave-before`.

## Adjacent observation

Existing cleave policy publishes a private destination leaf when a foreign crate
must name it. The negative regression therefore uses a public child beneath a
private ancestor, which remains inaccessible and is correctly refused. A separate
probe of leaf publication also showed the root public re-export disappearing when
publication and caller repair shared `lib.rs`. That overlap is outside the missing
Names dependency graph fix; the public-destination regressions preserve the
re-export and compile. Production visibility and publication policy are unchanged.
