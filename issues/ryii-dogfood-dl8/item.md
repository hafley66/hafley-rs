---
created: 2026-10-02
updated: 2026-10-02
type: bug
status: open
priority: normal
labels: [ryi]
---

# ryii dogfood on dl8 Rust: fast rename misses #[path] imports, rename lacks --verify, cleave mod.rs convention, extraction scope, log level

## Description

Found dogfooding `ryii` on sprefa dl8 Rust (2026-10-02). Full record: sprefa
`plans/v8/2026-10-02-ryii-dogfood.findings.md` (local main `02485960c`), branch
`chore/ryii-dogfood` (worktree `~/projects/sprefa-wt/ryii-dogfood`, commits `34c2b6b38`,
`66c92ea90`, `4240b5233`).

## Acceptance Criteria

- [ ] **Fast rename misses imports through a `#[path]` module.** `ryii rename 'src/_2_lower/_7_goals.rs#term_mentions' goal_mentions` (fast tier, no index) planned edits to `_7_goals.rs` only, `{"abstains":[]}`, exit 0. It missed 3 sites in `src/_2_lower/_7a_tuple_positions.rs`: `use super::execute::{term_has_var, term_mentions};` plus 2 calls, where `mod.rs` declares `#[path = "_7_goals.rs"] pub mod execute;`. Applying the plan breaks the build. The slow tier (`--verify-scip index.scip`) reported `scip-merge 3 sites`, `disagreements=3` and was correct. Fix: resolve `use super::<mod>::{name}` through `#[path]` module declarations; when resolution is uncertain, abstain (exit 7) instead of exit 0.
- [ ] **`rename` has no `--verify <cmd>`.** `cleave` and `move` both take `--verify` with rollback; `rename` does not. Add it.
- [ ] **cleave ignores the repo's `mod.rs` convention.** `ryii cleave 'src/_2_lower/_7_execute.rs#tuple_position_rules' src/_2_lower/_7a_tuple_positions.rs --drag` appended `pub mod _7a_tuple_positions;` at the end of `src/_2_lower/mod.rs`. The repo pattern is `#[path = "_12_units.rs"] pub mod units;`, in numeric order. Emit `#[path = "<file>"] pub(crate) mod <name-without-prefix>;` and insert it in order when siblings use that pattern.
- [ ] **Extraction scope is the whole git root.** Every dry run extracts `v5/`, `v6/`, `labs/` and others, about 6 s each, for an edit inside `src/`. Add `--scope <dir>`, or default to the nearest Cargo package or `package.json`.
- [ ] **Default log level floods stderr.** INFO prints one `extract_file … close` line per file. Default to `warn`.

## Tests Run

## Implementation Notes
