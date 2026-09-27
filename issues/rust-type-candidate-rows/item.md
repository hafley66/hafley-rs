---
created: 2026-09-26
updated: 2026-09-26
type: bug
status: fixed
priority: normal
labels: [scm, rust]
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
