---
created: 2026-08-24
updated: 2026-08-24
type: improvement
status: open
priority: high
epic: boop-one-path
labels: [domain-boop]
size: M
---

# codex-acp is the only codex launcher

## Description

## Description

`crates/boop-harness/src/harness/codex.rs` (`codex exec`) and `crates/boop-acp/src/channel/codex.rs` (`codex-acp`) both launch codex; the sandbox fix landed twice (ffd007b, 4d5a5f4).

Cut: delete the `codex exec` path; `boop tui codex` and lanes both go through the ACP channel.

## Acceptance Criteria

- [ ] `grep -rn "codex exec" crates/` returns nothing
- [ ] codex lane e2e (TASKS/codex-native-messaging.REPORT.md chain) green
