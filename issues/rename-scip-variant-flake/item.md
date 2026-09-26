---
created: 2026-09-25
updated: 2026-09-26
type: bug
reporter: claude-lane-w
status: open
priority: normal
---

# 5_rename_rust scip_verify_agrees_on_the_variant_fixture flakes under the full suite

## Description

Full cargo test --features cli run (216 binaries, ryi/write-side after the edit split): scip_verify_agrees_on_the_variant_fixture failed with 'scip-verify src/lib.rs:42-45 plan-only, disagreements=1' against an index the test builds with rust-analyzer in a temp dir. The same test passed 3/3 alone and the whole 5_rename_rust binary passed on rerun (17 passed). The plan (3 uses in lib.rs, 2 in uses.rs) matches; the fresh index lacked one occurrence. Suspect: rust-analyzer scip under parallel load (concurrent indexers).

## Comments

### 2026-09-26T04:16:08Z · @codex

2026-09-26 W2 evidence: src/lib.rs byte 42..45 is the Old enum declaration. The reported plan-only row therefore means the generated SCIP document lacked the exact DEFINITION occurrence used by anchor_symbol, or the definition carried a different range or role. ScipRust.build runs rust-analyzer scip on a staged copy and accepts its successful exit; it does not validate occurrence completeness. Sixteen isolated test processes launched with eight concurrent rust-analyzer indexers all passed (first wave 29.9–31.2 s each, second 9.9–11.2 s). The full 1057-pass crate gate also passed this test. The load trigger and index contents from the failed run were not captured, so the cause is not established and the issue remains open.
