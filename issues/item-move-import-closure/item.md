---
created: 2026-09-19
updated: 2026-09-19
type: feature
status: open
priority: high
epic: extract-parity-move-rename
related: ['@capability-as-data', '@rename-path-double-reach']
labels: [extract, artifact-cli]
---

# move one item between modules with its import closure

## Description

## Description

User-set 2026-09-19, wanted ASAP: take an item out of one module and put it in
another, carrying the specifiers it closes over, deleting the ones the source
file no longer needs. The verb does not exist and every piece it needs does.

Proposed spelling: `ryi cleave`. `ryi split` is the plain alternative.

### What exists today

| verb | granularity | repairs |
| --- | --- | --- |
| `ryi move OLD NEW` | whole files and folders | every specifier naming the file; `--relocate-mod`; `--shim` |
| `ryi rename FILE#OLD NEW` | one bound symbol, in place | every occurrence bound to it, declaration does not move |
| `ryi region TARGET ID` | one generated comment region | nothing else |

Searched 2026-09-19, zero hits across `crates/sprefa-extract/src/`:

```
unused_import / prune_import / drop_unused / dead_import   0 files
move_item / move_symbol / extract_item / hoist             0 hits
```

So nothing moves an ITEM, nothing carries its imports, and nothing prunes the
imports a removal orphaned.

### The algorithm

For item `I` in source file `F` moving to destination `D`, the closure is the
free names of `I` resolved against `F`'s module scope.

```
step 0  refs_out(I) = names I references that I does not declare
step 1  partition refs_out(I) by where F binds them:
          (a) declared in F      -> either drags along, or F re-exports for D
          (b) imported into F    -> the specifier travels to D
          (c) crate/std path     -> the specifier travels to D
step 2  remove I from F
step 3  for every specifier s in F: if no remaining reference in F binds to s,
        DELETE s
step 4  in D: add every specifier from (b) and (c) that D does not already
        carry, deduped against D's existing set
step 5  case (a) items that nothing else in F uses are candidates to drag;
        re-run from step 0 over the dragged set
step 6  fixpoint when the dragged set stops growing
```

Terminates at a fixpoint, not a base case. Step 5 is the part that makes this
useful for splitting a file: naming one item pulls its private helpers with it.

### Every input already exists

| need | emitted today |
| --- | --- |
| every item's span | `type` plane `node` rows |
| every reference to an item | `resolved_edge`, `resolved_type_edge` |
| which specifier feeds which reference | `resolved_import`, `SpecifierKind` |
| whether a name is still referenced in F | the same edges, counted per file |
| atomic apply with rollback | `move --commit --verify`, Soopy staging |
| Rust `mod` wiring | `move --relocate-mod` |
| text-level leftovers | `--text-refs` |

The gap is a PLANNER over relations the crate already produces, plus edits on
two files instead of one. The apply machinery is `move`'s and is reused whole.

### Why now

`crates/sprefa-extract/src/` is 67902 lines across 101 files:

```
5138  src/lang/ts.rs
4876  src/lang/go.rs
3906  src/types.rs
3624  src/lang/python/_0_source.rs
3450  src/lang/rust.rs
2791  src/project.rs
2162  src/lang/rust_rename.rs
2135  src/lang/kotlin.rs
```

`types.rs` at 3906 holds all fourteen traits plus every vocabulary enum, which
is why @capability-as-data and any other trait work collide there. Splitting by
hand is the thing the tool is supposed to remove.

### The layout this unlocks

User-set 2026-09-19: one folder per language, and the files under it named for
the trait being implemented, helpers at the top.

The convention is already half-adopted. These are folders:

```
src/lang/{commonlisp,data,gdscript,markdown,prolog,python}/
```

These are flat files with a name-prefix convention instead:

```
src/lang/rust.rs, rust_checker.rs, rust_checker_ra.rs, rust_docs.rs,
rust_mbe.rs, rust_modules.rs, rust_receivers.rs, rust_rehome.rs,
rust_rename.rs, rust_scip_macros.rs, rust_type_edges.rs, rust_type_refs.rs
src/lang/ts.rs, ts_checker.rs, ts_paths.rs, ts_receivers.rs, ts_rehome.rs,
ts_rename.rs, ts_resolve.rs
```

Target shape, numbered by dependency order per the repo's filesystem
convention, with `mod.rs` unnumbered because index files take no number:

```
src/lang/rust/
  mod.rs               re-exports only, no number
  0_parser.rs          impl Parser
  1_project_cst.rs     impl Project<CstF>
  2_project_type.rs    impl Project<TypeF>
  3_project_call.rs    impl Project<CallF>
  4_project_df.rs      impl Project<DfF>
  5_source.rs          impl Source
  6_resolve_call.rs    impl Resolve<CallF>
  7_resolve_type.rs    impl Resolve<TypeF>
  8_rehome.rs          impl Rehome
  9_rename.rs          impl Rename
```

One file per trait impl makes @capability-as-data readable from the filesystem,
and makes the cargo-feature-per-language cut a directory-level choice.

That layout migration is a SEPARATE issue and is the first real customer of
this verb. Do not do both in one lane.

## Acceptance Criteria

- [ ] a verb moves one named item from one file to another, dry-run by default
- [ ] the item's free names are resolved against the source module's scope, not guessed by text
- [ ] specifiers the item needs are added to the destination, deduped against what it already carries
- [ ] specifiers the removal orphaned are deleted from the source, and only those
- [ ] a private helper that nothing else in the source uses is offered as a drag candidate
- [ ] the drag set reaches a fixpoint and the termination is asserted, not assumed
- [ ] `--commit` reuses `move`'s staging, `--verify` and rollback unchanged
- [ ] a Rust move adds or relocates the `mod` wiring the way `move --relocate-mod` does
- [ ] `--text-refs` reports the spellings the move leaves behind in plain text
- [ ] a fixture proves: move one item, source loses exactly the orphaned specifiers, destination gains exactly the needed ones, crate still compiles
- [ ] a refusal is never the answer; an unresolvable free name prints what it can plus the commands that would answer it
- [ ] `cargo test --features cli --no-fail-fast`, zero failures
