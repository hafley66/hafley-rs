---
created: 2026-09-27
updated: 2026-09-27
type: chore
status: open
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

- [ ] Upgrade active `syn` dependencies and compatible feature sets to 3.x
- [ ] Migrate parser and visitor call sites in every affected crate
- [ ] Run workspace and full `sprefa-extract` suites with zero failures

## Split Rationale

Split from `@dep-bump-frontends` on 2026-09-27 so the major AST migration is
reviewed separately from the OXC and rust-analyzer front-end version updates.
