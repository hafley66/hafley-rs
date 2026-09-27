---
created: 2026-09-18
updated: 2026-09-27
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

## Plan

Run the existing ratchet command with `RATCHET_BUMP=1` against the named sprefa bench corpora, record true, wrong-target, and unresolved counts by language and resolution origin beside the plan, verify each language has nonzero join coverage, and open a scoped follow-up for every wrong-target class before accepting updated counts.

## Decision

Decision: nonzero wrong-target rows block ratchet updates until each class has a follow-up issue.

## Queue receipt

Measurement gate remains unchecked per the 2026-09-27 queue instruction. Exact measurement command: from `~/projects/sprefa`, `RATCHET_BUMP=1 just extract-ratchet`. It runs the six SCIP ratchet legs; no CodeQL command is part of this gate. The code gate will reject a ratchet update while any wrong-target count is nonzero.

Code-gate receipt: `pin_ratchet_tsv` checks every nonzero `(language, origin)` wrong-target class before writing `RATCHET.tsv`. Each class requires a matching `issues/real-repo-ratchet-wrong-target-<lang>-<origin>/item.md` with status `open`, `in_progress`, or `fixed`, the exact class title, and a reproduction receipt. `ratchet_bump_requires_a_followup_card_for_each_wrong_target_class` verifies missing, valid, and obsolete-card behavior. Full verification passed: sprefa 1,122 passed / 18 skipped; workspace 1,378 passed / 203 skipped. The measurement gate remains unchecked.
