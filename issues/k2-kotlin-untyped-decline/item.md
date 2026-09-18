---
created: 2026-09-18
updated: 2026-09-18
type: bug
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
lane: extract-resolver
blocked_by: ['@k1-kotlin-receiver-plane']
---

# K2: kotlin name-match legs decline untyped member receivers

## Description


Lane D law for kotlin: a navigation call whose receiver K1 could not type gets no same_file, module_plane, or corpus_unique answer; drop reason inferred. Move `shadowed()` (kotlin.rs) off output.df onto the K1 receiver rows so it works under --family call. Reasons per D.2: inferred / no_corpus_def / ambiguous.

## Acceptance Criteria
- [ ] tests/138_untyped_receiver_kotlin.rs mirrors tests/135 and 136
- [ ] CTF on a kotlin corpus: before/after origin and reason histograms in the report
- [ ] full crate gate green
