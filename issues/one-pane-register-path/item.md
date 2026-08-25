---
created: 2026-08-24
updated: 2026-08-25
type: improvement
status: open
priority: high
epic: boop-one-path
labels: [domain-boop]
size: S
---

# One pane-register path: tui <harness> and agent register

## Description

## Description

`tui`, `codex`, `shell-init`, `me`, `beep agent register` all register a pane or a pane-less row.

Cut: `tui <harness>` for panes, `agent register <name>` for pane-less; the rest hidden aliases.

## Acceptance Criteria

- [x] `codex`, `shell-init`, `me` hidden
- [ ] one registry write function

## Agent Runs

### 2026-08-25T04:18:37Z · @chore-verb-cuts

06a0d12 codex, shell-init, me clap variants hidden = true; doctrine REGISTER section points to boop tui <harness> and boop beep agent register <name>. one registry write function is out of scope: those verbs' bodies live outside crates/boop/src/main.rs and crates/boop/src/cli/mod.rs, owned by other lanes.
