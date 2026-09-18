---
created: 2026-09-18
updated: 2026-09-18
type: bug
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
lane: extract-tests
---

# RATCHET.tsv pin skips origins absent from the run

## Description


tests/golden_parity.rs:2012 iterates the run's by_origin; an origin whose rows all vanish (rust same_file 2 -> 0) skips its floor, so a floor is dodgeable by dropping the origin. unresolved column is never asserted; the RATCHET_BUMP path never updates it for existing rows. Fix: iterate the pinned rows for the lang, treat a missing origin as 0 true, assert the floor; assert unresolved ceiling too.

## Acceptance Criteria
- [ ] a run with an origin missing fails the floor
- [ ] unresolved is a ceiling
