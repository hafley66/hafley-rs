---
created: 2026-09-18
updated: 2026-09-18
type: chore
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
lane: extract-oracle
blocked_by: ['@k2-kotlin-untyped-decline', '@ratchet-pin-missing-origin']
---

# K3: scip-java oracle and a kotlin scip ratchet

## Description


scip_ensure.rs:92-97 declares scip-java (markers build.gradle.kts / build.gradle / pom.xml) but nothing is installed: no JDK, no coursier. Install JDK + coursier + scip-java, add a gradle fixture project under tests/fixtures/kotlin/ that scip-java can index, add call_resolve_scip_ratchet_kotlin to tests/golden_parity.rs (fourth copy of the ts/go/rust pattern; factor the origin-join block first, see review row 7), pin kotlin rows in RATCHET.tsv.

## Acceptance Criteria
- [ ] `extract slow` over the kotlin fixture produces scip facts (5_scip_facts_cli covers it)
- [ ] RATCHET.tsv has kotlin rows; wrong_target 0
- [ ] join_hits > 0 on the kotlin ratchet
