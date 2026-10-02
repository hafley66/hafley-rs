# alloy-scm Rust vs hafley-tsp Rust

A: read-only /Users/chrishafley/projects/hafley-tsp/packages/rust, hand-written Alloy Rust on @alloy-js/core 0.23.0-dev.12. B: this isolated lab, generated from hafley_scm's tree-sitter-rust 0.24.2 grammar.json and node-types.json plus hand Rust locals.scm and policy.

Requested denominator: **41 PASS, 0 DIFF, 96 GAP; 137/137 inventoried**. Including declaration renders and other matchers: **111 PASS, 15 DIFF, 113 GAP; 239/239 inventoried**. Strip-whitespace comparison (remove every `\s` character from measured strings): **41 PASS, 0 DIFF, 96 GAP** primary; **122 PASS, 4 DIFF, 113 GAP** all matchers. Scalar and presence matchers retain their measured status.

Before/after totals (239 matchers; PASS/DIFF/GAP):

| step | byte PASS/DIFF/GAP | strip-whitespace PASS/DIFF/GAP |
|---|---|---|
| Before | 87/30/122 | 101/16/122 |
| 1. Comparison columns | 87/30/122 | 101/16/122 |
| 2. Printer choice | 96/21/122 | 108/9/122 |
| 3. Named Rust props | 97/20/122 | 108/9/122 |
| 4. Module registry | 111/15/113 | 122/4/113 |

Primary baseline: **30/5/102** for both comparisons; current primary: **41/0/96** byte and **41/0/96** strip-whitespace.

All 27 A test files have B twins. A has 101 inline snapshots and 36 toBe assertions (137), plus 85 toRenderTo, 9 toBeNull, 6 toContain, and 2 toEqual assertions (102).

## Run

`pnpm install --ignore-workspace && pnpm test`

Prerequisites: Node/pnpm, Cargo with the worktree grammar dependencies already fetched, rustfmt, and the existing ryii CLI (tree-sitter query). No boop or hafley-rs Cargo tests run. Generation uses cargo metadata, without building a crate. Tests use the existing dependency installation through an ignored local symlink in this run; a fresh pnpm install creates local dependencies.

`pnpm test` regenerates Rust, type-checks the lab, clears stale measurements, runs all twins, verifies the assertion inventory, checks emitted Rust, and regenerates this README and the reports. Exit 0 means the measurement harness completed; PASS/DIFF/GAP and syntax results are the comparison result. It does not require every parity assertion to pass.

To refresh twins after a deliberate change in A: `node 3_twins.mjs /path/to/hafley-tsp/packages/rust/src`. Original source is copied byte-for-byte to fixtures/ and hashed. Expected literals are copied verbatim to 3_assertions.json and runnable twins. Unsupported pipeline twins retain inputs and expected literals in their linked descriptors and fixtures; they explicitly record GAP rather than executing A.

## Reuse and boundaries

0_gen.mjs and core/ were copied from ../the-gang-tries-to-make-alloy-turnkey. The lab-local core/0_print.tsx now diverges to retain separator and terminator literals. The generator resolves policy/subset/locals beside itself, so copying keeps B isolated. All 163 named concrete Rust kinds are selected. The lab-local generator also diverges to lower Rust policy-selected where_clause and mutable_specifier children into named props. The fallback policy remains byte-equal to the source lab. No edits to A or the turnkey lab.

2_components.tsx is a hand-written test adapter from A's high-level declaration props to generated nodes. Attribute strings, raw type/expression children, self parameters, async modifiers, and field/variant trailing commas use caller text where the grammar exposes opaque children. These are counted as hand-written code. Successful cases therefore measure generated nodes plus this adapter and policy. Expected output never supplies candidate input.

The policy uses per-file declaration scopes and a reactive module registry. CrateDirectory and ModDirectory own child module sets; module-root SourceFiles emit sorted pub mod declarations. Cross-file references register deduplicated, sorted crate-qualified use statements and emit the symbol name at the usage site. The lab has no named-type/function symbol metadata factories, TypeSpec adapter, operation planner, file zones, or endpoint/routing/daemon pipeline. Related tests remain GAPs. No pipeline GAP is counted as a syntax or equality pass. Remaining strip-whitespace DIFFs are where clauses: A emits a trailing predicate comma and B omits it.

The printer selects complete prop consumption by maximizing comma literals between supplied values and semicolon terminators, then minimizing other literals. The Rust policy accepts the semicolon branch for tuple bodies and the braced branch for field bodies. This retains Rust list separators and tuple-struct terminators. Rust where_clause and mutable_specifier are consumed from named prop queues; visibility and opaque function modifiers remain children. The adapter supplies where clauses without an optional trailing comma.

## Validation

Every measured emitted string and every file produced by a multi-file render call has a tree-sitter-rust query for both ERROR and MISSING plus `rustfmt --check --edition 2021 --config skip_children=true`. skip_children prevents rustfmt from resolving modules into unrelated files. Default rustfmt indentation is preserved. Fragments are embedded in syntax contexts: reference types in a type alias, let bindings in a function, serde fragments as an attribute, documentation attached to an item, and name-policy outputs in an appropriate declaration. Each exact context is recorded. Scalar booleans/null/presence checks have no Rust text to parse. The original A expected text receives the same checks. No Rust compilation or type-check claim is made.

B: **120/120 contexts have 0 ERROR/MISSING**, 0 fail parsing; **66/120 pass rustfmt check**, 54 fail. Multi-file renders additionally produced 28 files, including unasserted outputs: 28/28 have zero ERROR/MISSING and 9/28 pass rustfmt check. A formatting and invalid raw-identifier fragment results remain available beside B results. Byte equality and syntax validity are recorded separately.

Tools used: rustfmt 1.10.0-nightly (17fd5b8a37 2026-08-28); ryii 0.1.0.

## Per A test file

Primary columns use the requested 137 denominator; all columns include supplemental matchers. Top GAP reason is computed across all matchers.

| A test file | primary assertions | PASS | DIFF | GAP | all assertions | PASS | DIFF | GAP | top GAP reason |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 00_name-policy.test.ts | 16 | 16 | 0 | 0 | 16 | 16 | 0 | 0 | none |
| adapters/00_typespec-to-neutral.test.ts | 13 | 0 | 0 | 13 | 13 | 0 | 0 | 13 | other |
| adapters/01_integration.test.tsx | 6 | 0 | 0 | 6 | 6 | 0 | 0 | 6 | other |
| components/0_primitives/2_Serde.test.tsx | 10 | 10 | 0 | 0 | 20 | 20 | 0 | 0 | none |
| components/1_declarations/0_StructDeclaration.test.tsx | 0 | 0 | 0 | 0 | 13 | 12 | 1 | 0 | none |
| components/1_declarations/1_EnumDeclaration.test.tsx | 0 | 0 | 0 | 0 | 7 | 7 | 0 | 0 | none |
| components/1_declarations/2_parity.test.tsx | 0 | 0 | 0 | 0 | 35 | 31 | 2 | 2 | doc comments |
| components/1_declarations/4_FunctionDeclaration.test.tsx | 0 | 0 | 0 | 0 | 11 | 2 | 9 | 0 | none |
| components/1_declarations/6_ImplBlock.test.tsx | 0 | 0 | 0 | 0 | 7 | 4 | 3 | 0 | none |
| components/2_references/0_Reference.test.tsx | 4 | 4 | 0 | 0 | 4 | 4 | 0 | 0 | none |
| components/3_files/0_SourceFile.test.tsx | 4 | 4 | 0 | 0 | 5 | 5 | 0 | 0 | none |
| components/3_files/2_ModDirectory.test.tsx | 7 | 7 | 0 | 0 | 10 | 10 | 0 | 0 | none |
| components/4_codegen/1_CodegenPair.test.tsx | 2 | 0 | 0 | 2 | 8 | 0 | 0 | 8 | refkey/import resolution |
| components/4_codegen/3_ReplaceFile.test.tsx | 5 | 0 | 0 | 5 | 5 | 0 | 0 | 5 | other |
| components/4_codegen/4_AxumEndpoint.test.tsx | 3 | 0 | 0 | 3 | 3 | 0 | 0 | 3 | other |
| components/4_codegen/6_Endpoint.test.tsx | 6 | 0 | 0 | 6 | 6 | 0 | 0 | 6 | other |
| components/5_tests/0_integration.test.tsx | 10 | 0 | 0 | 10 | 10 | 0 | 0 | 10 | other |
| components/5_tests/1_cargo-check.test.tsx | 1 | 0 | 0 | 1 | 1 | 0 | 0 | 1 | other |
| components/5_tests/2_axum-routing.test.tsx | 9 | 0 | 0 | 9 | 9 | 0 | 0 | 9 | other |
| components/5_tests/3_colocated-routing.test.tsx | 7 | 0 | 0 | 7 | 8 | 0 | 0 | 8 | other |
| components/5_tests/4_auto-manual-split.test.tsx | 7 | 0 | 0 | 7 | 7 | 0 | 0 | 7 | other |
| components/5_tests/5_endpoint-component.test.tsx | 6 | 0 | 0 | 6 | 6 | 0 | 0 | 6 | other |
| emitter/04_ops-plan.test.tsx | 0 | 0 | 0 | 0 | 1 | 0 | 0 | 1 | other |
| emitter/06_emit-ops.test.tsx | 6 | 0 | 0 | 6 | 9 | 0 | 0 | 9 | other |
| emitter/07_daemon-files.test.tsx | 5 | 0 | 0 | 5 | 5 | 0 | 0 | 5 | other |
| emitter/emitter.test.tsx | 8 | 0 | 0 | 8 | 8 | 0 | 0 | 8 | other |
| symbols/symbols.test.tsx | 2 | 0 | 0 | 2 | 6 | 0 | 0 | 6 | scopes/symbol tables |
| TOTAL | 137 | 41 | 0 | 96 | 239 | 111 | 15 | 113 | |

## Line counts

A implementation lines cover its full emitter, adapters, components, symbols, and scopes. B generated nodes cover grammar syntax; B's absent pipelines are listed above.

| category | path | lines |
|---|---|---:|
| hand Rust policy | rust/0_subset.mjs | 169 |
| hand Rust policy | rust/0_name-policy.ts | 13 |
| hand Rust policy | rust/1_scope.ts | 32 |
| hand Rust policy | rust/2_print.tsx | 39 |
| hand Rust policy | rust/3_SourceFile.tsx | 61 |
| hand Rust policy | rust/locals.scm | 17 |
| hand twin intent adapters | 2_components.tsx | 90 |
| generated Rust, 163 components | gen/rust/0_nodes.tsx | 1416 |
| copied generator | 0_gen.mjs | 252 |
| copied shared printer | core/0_print.tsx | 150 |
| copied fallback policy | core/1_plain.tsx | 9 |
| A hand implementation (54 files, excludes tests) | packages/rust/src | 3959 |
| A oracle test fixtures (27 files) | fixtures/ | 5365 |
| B twins (27 files) | twins/ | 1733 |

[Per-assertion outcomes and exact diffs](6_results.md), [machine-readable outcomes and syntax diagnostics](6_results.json), [immutable assertion inventory](3_assertions.json).
