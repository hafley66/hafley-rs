---
created: 2026-09-19
updated: 2026-09-19
type: feature
status: open
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

So every other seam re-declares its membership in a hand-maintained static
table, and a test rails the table against the roster:

| seam | table | declares capability statically? |
| --- | --- | --- |
| resolve | `RESOLVE_ARMS`, `src/project.rs:1703-1783` | YES, `Option<fn>` per arm per language |
| rehome | `rehomes()`, `src/lang/mod.rs:141-173` | YES, `Option<&dyn RehomeManifests>` and friends |
| rename | `renames()`, `src/lang/mod.rs:185-187` | YES, membership list |
| scip | `INDEXERS`, `src/scip_ensure.rs:62-105` | YES, one row per language |
| **phase-1 planes** | `sources()`, `src/lang/mod.rs:117-131` | **NO, a bare list with no columns** |

`Source::extract(path, content, mask) -> RyiOutput` returns five
`Option<FamilyBundle<_>>` fields (`types.rs:2700-2707`). Which planes a
language implements is decided at RUNTIME by which of those come back `Some`.
Nothing can read it without parsing a file.

That is the whole gap. Thirteen seams declare capability as data; one hides it
behind Option returns.

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
| `LANGUAGE COVERAGE` help text | `src/bin/ryi/help.rs:189-202` | NO, hand-written `const` |
| architecture matrix | `crates/sprefa-extract/docs/0_architecture-matrix-20260917.md:43-67` | NO, static markdown, stamped to commit `16ebd451` |
| `4_capability_parity.rs` | `tests/4_capability_parity.rs:132-205` | its 15 capabilities are crate-global, and every `reach_of` arm uses a TS fixture |

`tests/33_v5_parity_matrix.rs:3,53` cites `docs/v5-extraction-parity.md`, which
does not exist anywhere in the repo. The matrix lives only as a `const` inside
that test file.

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

- [ ] `Source` declares its planes without running `extract`
- [ ] `sources()` carries capability columns like `RESOLVE_ARMS` does
- [ ] one command prints the language x capability matrix: planes, resolve arms, rehome sub-traits, rename, checker, scip indexer
- [ ] a test asserts the printed matrix against every declared table, both directions
- [ ] a test asserts the matrix against `help.rs:189-202` so the help text cannot drift
- [ ] `docs/0_architecture-matrix-20260917.md` is generated or deleted, not hand-maintained
- [ ] the dangling `docs/v5-extraction-parity.md` reference in `tests/33_v5_parity_matrix.rs:3,53` is resolved
- [ ] the "row declares None while an impl exists" hole named at `tests/1_resolve_cli.rs:127-128` is closed or waived with a written reason
- [ ] a written evaluation records where a type is erased, where capability is re-declared by hand, and which of those are removable
- [ ] the cargo-feature-per-language cut is specified as a follow-up issue with the feature graph named
