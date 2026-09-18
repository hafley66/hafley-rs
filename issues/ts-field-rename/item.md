---
created: 2026-09-18
updated: 2026-09-18
type: feature
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
lane: extract-rename
blocked_by: ['@flash-review-cleanups']
---

# ts field rename: property seats typed by the receiver plane

## Description


ts_rename.rs renames scope-plane bindings only; a member access spelling `old` is a DynamicSeat (ts_rename.rs:409-457) and stops the run. Field rename: anchor a class field / interface property / object-literal key; collect every `recv.old` whose receiver the ts_receivers plane types to the owning class or interface (extend the walk to every StaticMemberExpression, today only call sites at visit_call_expression); also destructuring `{ old }`, shorthand props, `"old"` computed keys, type-level property signatures. Untyped receivers spelling `old` stay a stop (listed), overridable by a flag.

## Acceptance Criteria
- [ ] tests/4_rename_ts.rs gains field cases: same-file, cross-file importer, untyped receiver stop, destructuring
- [ ] tsc clean on the committed fixture tree (existing tsc_is_clean_on_the_committed_tree pattern)
- [ ] scip_verify agrees on the field fixture
