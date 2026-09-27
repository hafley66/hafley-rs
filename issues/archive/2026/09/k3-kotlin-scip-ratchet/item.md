---
created: 2026-09-18
updated: 2026-09-27
type: chore
status: fixed
priority: normal
epic: extract-parity-move-rename
labels: [extract]
lane: extract-oracle
blocked_by: ['@k2-kotlin-untyped-decline', '@ratchet-pin-missing-origin']
---

# K3: scip-java oracle and a kotlin scip ratchet

## Description


Initial repro: `scip_ensure.rs` declared the Gradle/Maven marker and scip-java path, but the Kotlin fixture produced `scip_skip` with `no_markers`; the machine also lacked a usable JDK runtime and coursier. Install the indexer toolchain, add a Gradle fixture under `tests/fixtures/kotlin/`, add `call_resolve_scip_ratchet_kotlin` to `tests/golden_parity.rs`, reuse the shared origin-to-edge join, and pin Kotlin rows in `RATCHET.tsv`.

## Acceptance Criteria
- [x] `extract slow` over the Kotlin fixture produces SCIP-resolved calls (`slow_kotlin_fixture_emits_scip_resolved_calls`)
- [x] `RATCHET.tsv` has Kotlin rows; `wrong_target` is 0
- [x] `join_hits > 0` on the Kotlin ratchet (`call_resolve_scip_ratchet_kotlin`)
- [x] Golden tests read committed `tests/fixtures/kotlin/scip/index.scip` and pass with `java`, `gradle`, and `scip-java` absent from PATH
- [x] `tests/fixtures/kotlin/scip/regen.sh` serializes regeneration through the `ryi-vs-codeql.lock` queue

## Plan

Prepare a small Gradle Kotlin fixture, generate and commit its SCIP index through the shared JVM queue, then share the current origin-keyed join logic with the Kotlin ratchet. Normal tests load the committed index, while `regen.sh` reproduces it. Pin only Kotlin rows whose fixture join coverage is nonzero and wrong-target count is zero.

## Decision

Use scip-java over the Gradle fixture through `regen.sh`; normal tests load the committed index. The origin-to-edge join uses the existing `origin_by_edge` and `origin_of` helpers (commit `49f4efe9`).

Receipt: `kotlin/corpus_unique` true=2, wrong_target=0, unresolved=0; `call_resolve_scip_ratchet_kotlin` passed. The two Kotlin tests passed with only Cargo tools on PATH and no Java/Gradle. Full `sprefa-extract` suite passed 1127/1127 (18 skipped); `scripts/ryi-e2e.sh /Users/chrishafley/.cache/boop/cargo-target/release` passed 14/14; workspace suite passed 1378/1378 (203 excluded, 1 leaky).

Verification: `cargo nextest run --features cli -j 2 --offline --locked --test all` passed 1127/1127 with 18 skipped; `scripts/ryi-e2e.sh /Users/chrishafley/.cache/boop/cargo-target/release` passed 14/14; `cargo nextest run --workspace -j 2 -E 'not (test(/e2e|live|tmux|tui_sigint|omp_live/))'` passed 1378/1378 with 203 excluded and 1 leaky test.

Toolchain: OpenJDK 25, Gradle 9.7.1, coursier 2.1.24, scip-java installed with `coursier install --contrib scip-java`.
