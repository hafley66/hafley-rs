---
created: 2026-09-18
updated: 2026-09-18
type: chore
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
lane: extract-tests
blocked_by: ['@ratchet-pin-missing-origin']
---

# Flash-lane review: weak tests and duplicated code

## Description


Twelve rows, all judged mid by the user 2026-09-18. Weak tests: tests/130 trait_bound cannot tell Proj::run from Widget::run (assert callee span); tests/134 three shadow tests share one uncaller-keyed drop assert, use.ts crossFileGeneric untested; tests/131 dupName passes without the module leg, Gadget.kt comment wrong. Duplication: golden_parity.rs origin-join block pasted 3x (~1029, ~1374, ~1707); kotlin_modules.rs names_by_package + package_scope duplicate declaring_file (:192); ts_receivers.rs locals stack parallels scope (seed Inferred for unannotated params instead); kotlin.rs module_target Option plumbing; ts load_type_params clears instead of stacks.

## Acceptance Criteria
- [ ] each row fixed or closed with a one-line reason
