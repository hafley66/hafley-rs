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

## Decisions

### 2026-08-25T04:38:23Z · @claude-5

shell-init stays visible: it is the human entry point that routes claude/codex/kimi/opencode through boop tui (bash_profile:164). 540a35f: boop tui stamps BOOP_SESSION/BOOP_LANE into the harness process so every boop call inside a TUI, its shell, and its native subagents is named without --as. The pane rung deleted in 843f0ae served this job; the env stamp replaces it.
