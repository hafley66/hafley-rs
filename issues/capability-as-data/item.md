---
created: 2026-09-19
updated: 2026-09-27
type: feature
status: fixed
priority: high
epic: extract-parity-move-rename
related: ['@kind-vocab-constraint', '@scip-ingestion-conformance', '@dep-bump-frontends']
labels: [extract, intent-architecture]
---

# capability as declared data, not runtime Option returns

## Description

## Description

User-set 2026-09-19: a complete evaluation of how the language seams are typed,
where capability leaks out of the type system into hand-maintained tables, and
what it would take for adding a language to be "impl these traits from one
crate, then pick your languages with cargo features" the way ast-grep packages
its grammars.

### The mechanism that forces every table

Fourteen traits carry the language seams:

| trait | types.rs:line | what it is |
| --- | --- | --- |
| `Family` | `:173` | the per-family node/edge vocabulary |
| `Parser` | `:1843` | arena + `parse<'a>` with a GAT `Parsed<'a>` |
| `Project<F: Family>` | `:1862` | phase 1: one parse, masked projection into a `FamilyBundle<F>` |
| `BlobSource` | `:1874` | bytes in, content id out |
| `Resolve<F: Family>` | `:2357` | phase 2: cross-file `ProjectEdge<F>` |
| `ScipSource` | `:2661` | one indexer behind a subprocess seam |
| `Source` | `:2712` | `name / matches / extract / extract_lang` |
| `Rehome` + 4 optional sub-traits | `:2780, :2816, :2826, :2833, :2839` | what a language answers when a file moves |
| `Rename` | `:2962` | bound-symbol rename |

The seams ARE traits. The capability problem is downstream of one fact: the
roster holds `&'static [&'static dyn Source]` (`src/lang/mod.rs:117-131`), and
a `dyn Source` cannot be asked whether its concrete type also implements
`Resolve<CallF>`. Trait objects erase every trait but the one they name.

At discovery, the other seams re-declared membership in hand-maintained static
tables, and tests railed those tables against the source roster:

| seam | table | declares capability statically? |
| --- | --- | --- |
| resolve | `RESOLVE_ARMS`, `src/project.rs:1703-1783` | YES, `Option<fn>` per arm per language |
| rehome | `rehomes()`, `src/lang/mod.rs:141-173` | YES, `Option<&dyn RehomeManifests>` and friends |
| rename | `renames()`, `src/lang/mod.rs:185-187` | YES, membership list |
| scip | `INDEXERS`, `src/scip_ensure.rs:62-105` | YES, one row per language |
| **phase-1 planes** | `Source::planes()`, queried through `sources()` | YES, each roster entry declares its mask |

`Source::extract(path, content, mask) -> RyiOutput` returns five
`Option<FamilyBundle<_>>` fields (`types.rs:2700-2707`). Which planes a
language implements is decided at RUNTIME by which of those come back `Some`.
Nothing can read it without parsing a file.

At discovery, the phase-one roster was the remaining gap. Since 2026-09-27,
`Source::planes()` declares those masks without running `extract`.

### The rail admits its own hole

`tests/1_resolve_cli.rs:127-128`, verbatim:

```
What it does not catch: a row that declares None while an impl exists.
```

Because the check is table-against-roster, not table-against-impls. Rust
cannot enumerate impls at runtime, so the table is the only source and a stale
`None` reads as a deliberate decline.

### What exists instead of a generated matrix

| artifact | file:line | gated by a test? |
| --- | --- | --- |
| `LANGUAGE COVERAGE` help text | `ryii --help` | generated from the capability rows at startup |
| architecture matrix | `crates/sprefa-extract/docs/0_architecture-matrix-20260917.md` | deleted 2026-09-27 |
| `4_capability_parity.rs` | `tests/4_capability_parity.rs:132-205` | its 15 capabilities are crate-global, and every `reach_of` arm uses a TS fixture |

`tests/33_v5_parity_matrix.rs` carries the v5 relation mapping and asserts it
against the checked-in captures and current schema.

The root `justfile` carries zero recipes touching this crate
(`grep -niE "ryi|extract|sprefa" justfile` exits 1).

### The shape wanted

1. `Source` declares its planes: `fn planes(&self) -> FamilyMask`, or an
   associated const. `sources()` then becomes a table with columns like every
   other seam, and the matrix is generated rather than written.
2. One command prints the full language x capability matrix: planes, resolve
   arms, rehome sub-traits, rename, checker tier, scip indexer, ratchet rows.
   No ad-hoc script.
3. A test asserts the generated matrix against the declared tables in both
   directions, and against the help text so `help.rs:189-202` cannot drift.
4. Adding a language is: implement the traits, add one roster row per seam,
   and the matrix updates itself.

### Conditional compilation per language

User-set 2026-09-19: the end state is a crate where the language set is a
cargo-feature choice, the way ast-grep packages grammars. Today every grammar
and front-end is unconditional, so a consumer that wants TS only still links
kotlin, go, prolog, gdscript, commonlisp and every tree-sitter grammar behind
them.

That is a separate increment from the matrix and depends on it: the feature
graph can only be cut where capability is already declared as data.

Also user-set: this crate embeds ast-grep and tree-sitter as ordinary
dependencies, so its consumers should be able to reach those APIs through it
rather than re-adding them. And when the sprefa compiler is running again, this
tool feeds it, watching files and producing the facts.

## Acceptance Criteria

- [x] `Source` declares its planes without running `extract`
- [x] `sources()` exposes each source's plane column alongside the trait object
- [x] `ryii capabilities` prints the language x capability matrix: planes, resolve arms, rehome sub-traits, rename, checker, SCIP indexer
- [x] a test asserts the printed matrix against every declared table in both directions
- [x] a test asserts the generated matrix in CLI help stays aligned with the command output
- [x] `docs/0_architecture-matrix-20260917.md` is deleted
- [x] the stale v5 parity document reference is resolved
- [x] the "row declares None while an impl exists" hole is closed by comparing each declared plane to isolated extraction output
- [x] a written evaluation records type erasure, hand-maintained capability tables, and which declarations can be removed
- [x] the cargo-feature-per-language cut is specified in `issues/per-language-cargo-features/item.md` with the current feature graph

## Decisions

Decision: ship the capability matrix as this increment. Defer the per-language Cargo feature cut to its own follow-up issue.

### Original repro · 2026-09-27

Repro on the current `ryii`: `ryii --help` lists no `capabilities` command. `Source` has five optional family outputs and no plane declaration; the language roster is a plain slice of source trait objects. The matrix command and a roster-to-implementation rail are absent.

### Implementation evaluation · 2026-09-27

The type erasure point remains `sources() -> &'static [&'static dyn Source]`:
the roster preserves only the `Source` vtable. `Source::planes()` makes the
phase-one plane column object-safe and available without calling `extract`;
each concrete source declares its own mask. The matrix joins that roster to
`RESOLVE_ARMS`, `rehomes()`, `renames()`, `CHECKER_TIERS`, and `INDEXERS`.
Those other rosters remain because their trait objects represent distinct
extension traits and optional sub-traits. Removing their parallel declarations
would require a shared generated/concrete registry that keeps all those trait
implementations together. The phase-one column no longer needs a hand-written
parallel table. The roster test calls each source with one plane mask at a
time and checks declared presence against its `RyiOutput`, closing the `None`
hole without making capability discovery parse a file.

The old architecture matrix was deleted. The relation mapping remains in
`tests/33_v5_parity_matrix.rs`, which owns its checked assertions and no longer
points at the removed `docs/v5-extraction-parity.md`.

### 2026-09-27 · @codex

`ryii capabilities` JSONL and `ryii --help` contain the same 12-row matrix;
`t_4_capability_parity::capabilities_matrix_matches_every_roster_and_help_table`
and `t_4_capability_parity::every_roster_source_is_reachable_through_the_binary`
pass. Gates: workspace 1366 passed; sprefa-extract 1119 passed.
