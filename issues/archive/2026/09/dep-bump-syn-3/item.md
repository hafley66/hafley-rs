---
created: 2026-09-27
updated: 2026-09-27
type: chore
status: fixed
priority: normal
epic: extract-parity-move-rename
labels: [extract, dependencies]
---

# migrate Rust AST consumers from syn 2 to syn 3

## Description

The `dep-bump-frontends` card splits the major `syn` upgrade from OXC and
rust-analyzer patch/minor upgrades. `syn` is used by `sprefa-extract`,
`hafley_scm`, `boop`, `hafley-observe`, and macro crates. A major parser AST
change can affect visitors and pattern matches across those consumers.

## Acceptance Criteria

- [x] Upgrade active `syn` dependencies and compatible feature sets to 3.x
- [x] Migrate parser and visitor call sites in every affected crate
- [x] Run workspace and full `sprefa-extract` suites with zero failures

## Split Rationale

Split from `@dep-bump-frontends` on 2026-09-27 so the major AST migration is
reviewed separately from the OXC and rust-analyzer front-end version updates.

Baseline repro (2026-09-27): `cargo tree -i syn@2 --workspace --depth 2` shows
the five active workspace consumers still use syn 2.0.119; syn 3.0.3 is only
transitive.

## Completion receipt · 2026-09-27 · @codex

Migrated all five direct consumers to `syn` 3 and updated the Rust type, guard,
receiver, trait, and generic-default AST call sites. Workspace gate passed:
1,366 passed, 203 skipped. Full `sprefa-extract` gate passed: 1,119 passed,
18 skipped. The Rust tree/syn DF parity test passes.
