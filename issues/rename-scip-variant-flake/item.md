---
created: 2026-09-25
updated: 2026-09-25
type: bug
reporter: claude-lane-w
status: open
priority: normal
---

# 5_rename_rust scip_verify_agrees_on_the_variant_fixture flakes under the full suite

## Description

Full cargo test --features cli run (216 binaries, ryi/write-side after the edit split): scip_verify_agrees_on_the_variant_fixture failed with 'scip-verify src/lib.rs:42-45 plan-only, disagreements=1' against an index the test builds with rust-analyzer in a temp dir. The same test passed 3/3 alone and the whole 5_rename_rust binary passed on rerun (17 passed). The plan (3 uses in lib.rs, 2 in uses.rs) matches; the fresh index lacked one occurrence. Suspect: rust-analyzer scip under parallel load (concurrent indexers).
