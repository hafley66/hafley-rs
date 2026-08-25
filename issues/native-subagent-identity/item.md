---
created: 2026-08-25
updated: 2026-08-25
type: bug
reporter: claude-5
status: open
priority: high
epic: boop-one-path
---

# codex native subagent inherits the lane BOOP_SESSION so wait --me watches the wrong mailbox

## Description

A codex native subagent spawned inside a lane inherits `BOOP_SESSION=<lane>` from the process env. `boop wait --me` then watches the lane mailbox and rows addressed to the native's own route (e.g. `native-n1d`) sit with `to_timestamp` null.

Receipt: pid 34606 sat in `boop wait --me --wait-timeout 600` for 8+ minutes with 4 rows to native-n1d unread; `BOOP_SESSION=native-n1d boop wait --me --wait-timeout 5` returned all 4, exit 0. `boop beep agent register <name>` prints the export line but nothing makes the subagent run it. Blocks codex-acp-only-launcher AC 2 (pong leg).

## Acceptance Criteria
- [ ] a registered native route wins over the inherited lane stamp for `--me`
- [ ] cx-a/cx-b chain pong leg green with the shim binary
