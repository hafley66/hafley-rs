---
created: 2026-09-18
updated: 2026-09-27
type: task
status: needs-decision
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

## Plan

Run the existing ratchet command with `RATCHET_BUMP=1` against the named sprefa bench corpora, record true, wrong-target, and unresolved counts by language and resolution origin beside the plan, verify each language has nonzero join coverage, and open a scoped follow-up for every wrong-target class before accepting updated counts.

## Decision

Should nonzero wrong-target rows block the ratchet update until each class has a follow-up issue?

## Queue receipt

Measurement gate remains unchecked per the 2026-09-27 queue instruction. Exact measurement command when authorized: from `~/projects/sprefa`, `RATCHET_BUMP=1 just extract-ratchet`. It runs the six SCIP ratchet legs; no CodeQL command is part of this gate. Wrong-target policy remains undecided, so status stays `needs-decision`.
