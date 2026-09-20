---
created: 2026-09-19
updated: 2026-09-20
type: feature
status: open
priority: normal
epic: extract-parity-move-rename
related: ['@default-families-no-conditional', '@lab-scopegraph-queries']
labels: [extract, intent-architecture]
---

# grammar kind vocabulary as a checked constraint, not a substring guess

## Description

`crates/sprefa-extract/src/lang/astgrep.rs` answers "is this a name leaf" with a
substring guess: `kind.contains("identifier")` at
`crates/sprefa-extract/src/lang/astgrep.rs:171` (`is_identifier_leaf`, the only
`kind.contains(` call in this checkout (grep confirms it). A kind the grammar
never declares still passes the check as long as it contains the substring.
The exact-match call-site table (`CALL_KINDS`, `CALLEE_FIRST_KINDS`,
`0_call_kinds.rs`) and the `MODULE_CALLER` relocation this card's earlier draft
described are not present on this branch; that work landed on the unmerged
branch `chore/lang-kind-ids-and-kotlin-decline` (commits `6ca57c7e`,
`a0baf70e`, `a24a59b4`) and has not reached `feat/observe-sqlite`. Until it
merges, `MODULE_CALLER` is `crate::lang::python::MODULE_CALLER`, read directly
by `crates/sprefa-extract/src/project.rs:2414` and
`crates/sprefa-extract/src/wire.rs:386,390`. The goal stands regardless of
which branch carries the substring today: one typed enum of node kinds per
language, generated from each grammar's `node-types.json`, so a kind the
grammar does not declare is a compile error instead of a silent substring
match.

## Candidates

`crates/sprefa-extract/Cargo.lock` pins 28 `tree-sitter-*` packages (27 grammar
crates plus the `tree-sitter-language` trait crate; `grep -c '^name =
"tree-sitter-'` confirms the count). Every grammar crate checked
(`tree-sitter-go`, `tree-sitter-kotlin-sg`, `tree-sitter-prolog`,
`tree-sitter-commonlisp`, `tree-sitter-gdscript`) ships a `pub const
NODE_TYPES: &str = include_str!(".../node-types.json")` in
`bindings/rust/lib.rs`, and `tree-sitter 0.25.10` exposes
`Language::node_kind_for_id` / `Language::id_for_node_kind` at runtime
(`binding_rust/lib.rs:510`, `:518`). Downloads are crates.io lifetime /
90-day `recent_downloads` as of 2026-09-20.

| crate | last release | downloads | what it generates | build.rs or proc-macro | handles 28 grammars | verdict |
| --- | --- | --- | --- | --- | --- | --- |
| `type-sitter` + `type-sitter-gen` + `type-sitter-lib` (Jakobeha) | 0.10.1, 2026-05-14 | type-sitter 57,967 / 8,668; type-sitter-gen 67,674 / 8,761; type-sitter-lib 64,697 / 8,743 | one struct per named node kind with typed field accessors, an `AnyNode`-style enum, query capture types, from `node-types.json` | both: `type-sitter-proc` (proc-macro, regenerates every build) or `type-sitter-gen` (build.rs, cached); `type-sitter-cli` for a manual one-shot codegen | yes, generically: build.rs mode accepts a `tree_sitter_<lang>::NODE_TYPES` const directly, no grammar vendoring required, so it reaches all 28 the same way; nothing is pregenerated, one gen pass per grammar | candidate; alpha (0.10.1, API "subject to change"), repo tracks `tree-sitter` 0.26, this workspace pins 0.25, needs a compat check before adoption |
| `treesitter-types` + `treesitter-types-macros` + per-lang `treesitter-types-<lang>` (jeroenvervaeke) | 1.0.0 (core, per-lang crates), 2026-07-26 | core 9,576 / 8,459; per-lang crates each in the tens-to-low-thousands (e.g. `-rust` 7,686, `-go` 384, `-python` 88) | structs/enums per node kind (supertypes become enums, optional fields `Option<T>`, repeated fields `Vec<T>`), same node-types.json source; per-lang crates ship pregenerated bindings | both: `treesitter_types::codegen::emit_to_out_dir` in build.rs (recommended), or `treesitter-types-macros::generate_types!` | no; pregenerated crates exist for 22 of the 28 (`-rust`, `-go`, `-python`, `-c-sharp`, `-ruby`, `-toml`, `-markdown`, confirmed by crates.io lookup); no `treesitter-types-kotlin`, `-prolog`, `-commonlisp`, `-gdscript`, or `-bash` exist, so those 5 still need the generic build.rs/macro path anyway | candidate; younger (1.0.0 since 2026-07), small single-maintainer project (per repo page: 45 commits, 5 stars), pregenerated coverage doesn't reach kotlin-sg/prolog/commonlisp/gdscript/bash so the generic path is required either way |
| `tree-sitter-node-types` | unknown | unknown | unknown | unknown | unknown | reject; no crate by this exact name found on crates.io or via GitHub search, the task's starting point does not resolve to a real package |
| `rust-sitter` | 0.4.5, 2025-05-06 | 307,760 / 12,934 | a proc-macro DSL for **authoring** a tree-sitter grammar inside Rust source, then generating a typed AST from that Rust-defined grammar | proc-macro | no; wrong direction: it defines new grammars in Rust, it does not type an existing, externally-authored `node-types.json` the way our 27 vendored grammars ship one | reject; solves grammar authoring, not typed access to an already-published grammar's kind set, also over a year since last release |
| raw `tree-sitter-<lang>::NODE_TYPES` + `Language::node_kind_for_id` / `id_for_node_kind` | n/a (already a transitive dependency of every grammar crate above) | n/a | nothing by itself: a JSON string and two runtime lookup functions, not a typed enum | n/a | yes, trivially, since it is already in the dependency tree for all 28 | not a candidate on its own; this is the data source both the recommendation and the fallback build.rs parse, listed for completeness |

## Recommendation

`type-sitter-gen` in build.rs mode: it consumes each grammar's `NODE_TYPES`
const directly (no vendoring, no submodule), so it reaches all 28 grammars the
same way and stays current with whatever `tree-sitter-<lang>` version this
crate already pins, unlike `treesitter-types`'s pregenerated crates which stop
at 22 of 28 and still need the same generic path for kotlin-sg, prolog,
commonlisp, gdscript, and bash. Confirm the `tree-sitter` 0.26-vs-0.25 version
gap before pulling it in. Fallback only, if `type-sitter-gen` does not clear
that check: a 30-line build.rs that parses each grammar's `NODE_TYPES` JSON
into a `NodeKind` enum per language, hand-rolled, no external crate.

## Acceptance Criteria

- [ ] a `NodeKind` enum exists for every loaded grammar
- [ ] `crates/sprefa-extract/src/lang/astgrep.rs` carries no `kind.contains(` call
- [ ] the call-kind strings (the `0_call_kinds.rs` table, once that work merges) become enum variants, not string literals
- [ ] `MODULE_CALLER` moves out of `crate::lang::python`
- [ ] `cargo test --features cli` is green from `crates/sprefa-extract`
- [ ] no mention of v5 anywhere in this card

## Tests Run

## Implementation Notes

## Comments

## Decisions

### 2026-09-20T18:00:37Z · @chris

buy the library that auto-generates node-kind enums per language; no bespoke build; every reference to sprefa v5 is deleted, v5 is dead

