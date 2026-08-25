---
created: 2026-08-24
updated: 2026-08-25
type: improvement
status: done
priority: high
epic: boop-one-path
labels: [domain-boop]
size: M
closed: 2026-08-25
closed_by: claude-5
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

### 2026-08-25T16:32:21Z · @claude-5

cx-a5/cx-b5 chain on the installed binary (dafb888 + merge): ping m-0c9ca8b7 native-n2e->native-n1e taken, pong m-be61838c native-n1e->native-n2e taken, feature-cx-b5 rc=0, feature-cx-a5 rc=0, commits 6f11cd3 and f6d503f. AC 2 checked.


## Comments

### 2026-08-25T14:02:21Z · @claude-5

AC 2 (pong leg) traced to native-subagent-identity, fixed on main: wait --me refuses a lane stamp shared with native children; natives pass --as. Rerun the cx chain from the shim binary to check the box.
