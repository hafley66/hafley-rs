---
created: 2026-09-26
updated: 2026-09-26
type: bug
status: obsolete
priority: normal
labels: [scm, rust]
closed: 2026-09-26
disposition_note: 'Focused current-branch repro passed (type_candidates_drop_generic_parameters_and_keep_owner_reference_order: 1 passed, 30 skipped); the reported failure does not reproduce.'
---

# `hafley_scm` type candidate rows test fails on main

## Description

`crates/hafley_scm/tests/9_type_candidate_rows.rs` fails on main because it expects generic type parameters to appear as external type candidates. `type_candidate_rows` removes names declared by the item's generics before returning candidates. Align the fixture's expected rows with that rule while retaining the owner, order, name, and kind assertions for external references.

## Acceptance Criteria

- [x] the focused `9_type_candidate_rows` test passes
- [x] no `#[allow]` attributes are added

## Tests Run

`cargo nextest run -p hafley_scm -j 2 -E 'test(/^type_candidates_drop_generic_parameters_and_keep_owner_reference_order$/)'` passed.


## Implementation Notes

## Comments

### 2026-09-27T01:54:15Z · @codex

Repro receipt: focused type_candidates_drop_generic_parameters_and_keep_owner_reference_order test passed on the current branch (1 passed, 30 skipped).

### 2026-09-27T01:54:34Z · @intake

Reopened: Current focused repro passed; classify the report as obsolete under the current-ryii repro rule.

### 2026-09-27T01:54:37Z · @intake

Obsolete: Focused current-branch repro passed (type_candidates_drop_generic_parameters_and_keep_owner_reference_order: 1 passed, 30 skipped); the reported failure does not reproduce.



## Reopen Notes — 2026-09-26

_Add rationale for reopening here._
