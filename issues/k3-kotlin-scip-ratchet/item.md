---
created: 2026-09-18
updated: 2026-09-18
type: chore
status: needs-decision
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

## Plan

Prepare a small Gradle Kotlin fixture and confirm the existing scip-java marker path can index it, then share the current origin-keyed join logic with the Kotlin ratchet, run `extract slow`, and pin only Kotlin rows whose fixture join coverage is nonzero and wrong-target count is zero; keep the ratchet command and output reproducible from the fixture.

## Decision

Should the fixture use a preinstalled JDK, coursier, and scip-java lane, or check in a generated SCIP index so indexing does not require those tools?
