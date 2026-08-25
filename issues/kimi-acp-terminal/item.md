---
created: 2026-08-24
updated: 2026-08-24
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

- [ ] initialize advertises `terminal: true`
- [ ] the five terminal/* methods implemented in boop-acp, spawning under the lane cwd with the lane env
- [ ] kimi lane with TASKS/side-flash4.md brief lands two commits and reports them
- [ ] unit test per method with a fake agent
