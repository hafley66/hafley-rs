# Lab: can grammar-generated alloy Rust (alloy-scm) pass the hafley-tsp Rust emitter's tests?
Branch lab/20261002-alloy-scm-rust. Lab dir: labs/isolated/lab-20261002-alloy-scm-rust-vs-tsp-rust (naming rule: lab-YYYYMMDD-<what-is-compared>).

## Two sides
- A (reference): /Users/chrishafley/projects/hafley-tsp/packages/rust — hand-written alloy Rust components on
  @alloy-js/core 0.23.0-dev.12. 27 test files, 137 snapshot/toBe assertions:
  src/00_name-policy.test.ts, components/{0_primitives,1_declarations,2_references,3_files,4_codegen,5_tests},
  emitter/, adapters/, symbols/. READ ONLY.
- B (candidate): labs/isolated/the-gang-tries-to-make-alloy-turnkey — `0_gen.mjs <lang>` generates alloy components
  from tree-sitter grammar.json/node-types.json/locals.scm; C and TypeSpec have hand policy dirs (`c/`, `typespec/`:
  subset, name policy, scope, print Policy, SourceFile, locals). README documents mapping + printer semantics.
  Rust grammar is reachable through hafley_scm's tree-sitter-rust (grammarSource).

## Work
1. New isolated lab package (copy turnkey's package.json/tsconfig/vitest setup; reuse its 0_gen.mjs and core/ by
   relative import or copy, say which). Generate gen/rust/0_nodes.tsx. Write rust/ policy dir:
   0_subset.mjs, 0_name-policy.ts (Rust keywords -> r#), 1_scope.ts, 2_print.tsx (rustfmt-like spacing), 3_SourceFile.tsx, locals.scm.
2. For every A test file, write the B twin: same input intent, built from generated components, asserting
   A's exact expected text (copy A's inline snapshot strings). Do not change expected text to fit B.
3. Extra check on every B output: tree-sitter-rust parse 0 ERROR/MISSING; `rustc --edition 2021 --crate-type lib
   -Zparse-only` is nightly-only, so use `rustfmt --check --edition 2021` on the emitted text, or compile in a
   scratch crate where the A test also compiles.
4. Classify each A assertion: PASS (byte-equal) | DIFF (text differs; show diff) | GAP (B cannot express it).
   GAP reasons: refkey/import resolution (`use` emission), name policy, scopes/symbol tables, doc comments,
   attributes/derives, generics, whitespace/blank-line policy, other.
5. README.md: what was compared, how to run (`pnpm install --ignore-workspace && pnpm test`),
   table per A test file: assertions, PASS, DIFF, GAP, top gap reason; total N/137; line counts hand vs generated (like turnkey README).

## Rules
- Do not edit hafley-tsp or the turnkey lab. Isolated package (not in any pnpm workspace).
- No Python. Commit per step (scaffold+gen; policy; twins in batches; README). Messages end
  `Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`. Do not push.
- Do not run boop or cargo tests of hafley-rs crates.
