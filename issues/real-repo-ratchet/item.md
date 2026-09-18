---
created: 2026-09-18
updated: 2026-09-18
type: task
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
lane: extract-oracle
blocked_by: ['@ratchet-pin-missing-origin']
---

# Run the scip ratchets on the sprefa bench corpora

## Description


RATCHET.tsv is pinned on fixture corpora only. `just extract-ratchet` with RATCHET_BUMP=1 on /Users/chrishafley/projects/sprefa/plans/extract-bench-2026-08-29/RATCHET.tsv corpora; report true / wrong_target / unresolved per (lang, origin). This is the number that grades the heuristic legs against SCIP on real code (prior art says heuristic recall 0.3 to 0.5; measure ours).

## Acceptance Criteria
- [ ] histogram per (lang, origin) on the bench corpora committed next to the plan
- [ ] wrong_target rows above 0 each get an issue
