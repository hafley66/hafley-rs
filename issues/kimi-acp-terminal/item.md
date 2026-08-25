---
created: 2026-08-24
updated: 2026-08-25
type: bug
status: open
priority: high
epic: boop-one-path
labels: [domain-boop]
---

# kimi lanes have no shell: ACP client lacks terminal capability

## Description

## Description

Kimi lane chore-kimi-probe (model kimi-code/k3, 2026-08-25 03:5x) ran `git commit` through its Bash tool and got `ACP terminal capability is unavailable` (wire.jsonl tool.result isError). The boop ACP client (crates/boop-acp/src/channel/acp.rs initialize handshake) does not advertise `clientCapabilities.terminal` and does not serve `terminal/create`, `terminal/output`, `terminal/wait_for_exit`, `terminal/kill`, `terminal/release`. Kimi routes every shell command through the client terminal, so a boop kimi lane cannot run a shell at all. Codex and claude ACP agents run their own shells and are unaffected. Reference: i:acp skill, terminal methods.

## Acceptance Criteria

- [x] initialize advertises `terminal: true`
- [x] the five terminal/* methods implemented in boop-acp, spawning under the lane cwd with the lane env
- [x] kimi lane with TASKS/side-flash4.md brief lands two commits and reports them
- [x] unit test per method with a fake agent

## Agent Runs

### 2026-08-25T12:50:30Z · @feat-epic-wave-b

51b81d3 boop-acp serves terminal/create|output|wait_for_exit|kill|release and initialize advertises clientCapabilities.terminal=true; new crates/boop-acp/src/channel/terminal.rs (registry, byte-limited buffer, process-group kill) plus wire handlers in channel/acp.rs. Tests: 13 registry unit tests + 7 wire tests against a fake ACP agent; boop-acp 53 passed 0 failed 6 ignored; full suite boop/boop-acp/boop-harness/boop-proc/boop-store/boop-mux all green. Live receipt: kimi lane chore-kimi-terminal-receipt --preset k3 from the scratch tree-repo landed 0350274 and e20909d (two commits) and the scratch store carries commit/idle/done rows (commit 764c060..e20909d dirty=0, idle turn=end_turn head=e20909d, lane done rc=0).
