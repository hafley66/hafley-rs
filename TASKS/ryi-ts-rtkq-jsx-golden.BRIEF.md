# ryi TS: dl5 rtkq + JSX goldens (issue ryi-ts-rtkq-jsx-golden; branch feature/ryi-ts-rtkq-jsx)

Read issues/ryi-ts-rtkq-jsx-golden/item.md. dl5 reference (read only): ~/projects/sprefa/v5/examples/rtkq-op-recovery.dl,
examples/flow-jsx.dl, examples/openapi-sim/*, tests/it/string_fns.rs:351.
crates/sprefa-extract is its own workspace root: gate is `cd crates/sprefa-extract && cargo test --features cli`
(2 pre-existing failures in tests/golden_parity.rs: ported_facets_match_v5, rust_doc_parity — root-prefix oracle diffs; leave them).
Skill: sprefa-extract-add-language (planes, roster rails, frozen goldens) — read before touching the TS front-end.

1. Copy examples/openapi-sim/{openapi.json,components.tsx,hooks.ts} into a sprefa-extract test fixture.
2. ryii fast/--resolve on .tsx emits call-site facts (callee text, path, line, enclosing fn) — today it emits only
   free_name/local/occurrence/symbol for components.tsx. Also JSX facts: element name, attributes, parent element,
   enclosing component, for a tsx fixture with nested JSX (write one; flow-jsx.dl's fixture if it has one).
3. Golden: a test that derives, from ryii facts, the dl5 rtkq result: hook -> op via strip `use`, `^Lazy|(Query|Mutation)$`,
   lcfirst; linked = {getUser x2 (incl. Lazy), listUsers, createOrder}; orphan = deleteWidget; unhooked spec ops as dl5 says.
   Do it as SQL over `--sqlite` output or Rust test over JSONL — not Python.
4. REPORT.md: new record kinds + columns, golden rows, timing on hafley-rxjs packages (`ryii fast packages`) before/after.
Rules: Rust profile already set (dev opt 1, deps opt 3). CARGO_BUILD_JOBS=4. Commit per step; messages end
`Co-Authored-By: Claude Opus 5.5 (1M context) <noreply@anthropic.com>`. No push. No boop tests.
