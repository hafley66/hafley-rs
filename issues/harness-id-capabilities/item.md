---
created: 2026-08-22
updated: 2026-08-22
type: task
status: done
priority: high
epic: harness-interface
labels: [domain-boop, intent-implementation]
size: M
closed: 2026-08-22
---

# HarnessId + Capabilities replace 16 harness literals

## Description

## Description

`HarnessId` enum + `Capabilities` struct replace 16 string comparisons (plan §1 table). Sites: `boop-proc/src/lane.rs:343,360,365`, `boop/src/cli/job.rs:798`, `boop/src/cli/me.rs:121`, `boop/src/cli/control.rs:44`, `boop-store/src/_0_session_graph.rs:464,1722`, `boop-harness/src/harness/claude.rs:69`, `boop-acp/src/channel/tui.rs:106,279,359,431`. `SessionRef.harness` and `Route.harness` take the enum. Lane P1, opus, running 2026-08-22.

## Acceptance Criteria

- [ ] `grep -rnE '== *"(claude|codex|kimi|opencode)"' crates --include=*.rs | grep -v tests` prints 0 lines
- [ ] `Registry::get(HarnessId)` is total; test-only `Echo` harness registers with no other edit
- [ ] `cargo test --workspace` green except pre-existing `inbox_hooks::a_hail_during_a_long_turn…`

## Comments

### 2026-08-22T22:45:06Z · @fable

Landed in PR #47: 8d821db, 25dbfad. HarnessId in boop-store/src/harness_id.rs; Capabilities without model_prefixes/process_names; literal grep 0 after P3.
