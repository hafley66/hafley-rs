---
created: 2026-08-24
updated: 2026-08-25
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

- [x] `grep -rn "codex exec" crates/` returns nothing
- [ ] codex lane e2e (TASKS/codex-native-messaging.REPORT.md chain) green

## Agent Runs

### 2026-08-25T13:15:02Z · @feat-epic-wave-b

f1ba77a deleted launch_command and shell_quote from crates/boop-harness/src/harness/codex.rs (the last 'codex exec' spelling; spawn already ran supervisor_command over the ACP channel) plus the five tests that measured the string, and reworded the send_midflight and debug.rs comments. grep -rn 'codex exec' crates/ returns nothing. Suite: boop-harness 119 passed 0 failed 1 ignored; full run over boop/boop-acp/boop-harness/boop-proc/boop-store/boop-mux green. Live chain in the scratch store (--preset luna, branches cx-a4/cx-b4, natives n1d/n2d): feature-cx-a4 committed 577b746..1eff510, its native native-n1d spawned feature-cx-b4 which committed 1eff510..1646f3c, and native-n2d sent 'ping from native-n2d inside lane feature-cx-b4' to native-n1d.

### 2026-08-25T13:19:01Z · @feat-epic-wave-b

f1ba77a chain receipt, ping green and pong blocked outside this issue. Ping leg green in the scratch store: feature-cx-a4 (--preset luna) commit 577b746..1eff510, native-n1d spawned feature-cx-b4, feature-cx-b4 commit 1eff510..1646f3c then done rc=0, and m-687a5667 'ping from native-n2d inside lane feature-cx-b4' reached native-n1d. The pong leg never fired, and the cause is the identity ladder, not the launcher: a codex native subagent inherits BOOP_SESSION=feature-cx-a4 / BOOP_LANE=feature-cx-a4 from its lane, so 'boop whoami' inside it answers feature-cx-a4 and 'boop wait --me' watches the lane's mailbox instead of the native's. Measured: pid 34606 sat in 'boop wait --me --wait-timeout 600' for over eight minutes while four rows addressed to native-n1d had to_timestamp null, and the same 'boop wait --me' run with BOOP_SESSION=native-n1d returned all four at once. 'boop beep agent register <name>' prints the 'export BOOP_SESSION=<name> BOOP_LANE=<name>' line the brief never tells the subagent to run. Worth its own issue.

