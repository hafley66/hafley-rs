---
created: 2026-09-14
updated: 2026-09-19
type: feature
status: done
priority: normal
closed: 2026-09-19
closed_by: omp-2
---

# Choose recoverable TUI sessions from a revival picker

## Description

Preserved uncommitted candidate as archival commit d37eaecf1e0e72b70da6139fc929329214b955e0, ref archive/boop-cleanup-20260914/wip/feature/tui-revive. SessionDigest and revive_candidates add transcript summaries, a since window and candidate selection. Main already has a respawn loop; this picker remains absent. Port only this distinct behavior and its tests after reviewing identity semantics against current main. Snapshot is unvalidated; worktree removed after complete source capture.

## Delivered 2026-09-19

Ported onto main: `Door::tui_resume_args` per harness, `boop beep lane revive`
(name, `--dead`, `--list`, `--json`, `-y`, `--since`, `--socket`), the picker
(SessionDigest, revive_candidates, parse_selection, `--list` table and JSON),
the REVIVABLE mark in `lane list`, and socket-scoped liveness merged with main's
pane-pid batch path. Evidence: door `tui_resume_args` round trips, cli::control
unit tests, `tui_revive_e2e` (real claude/codex/opencode against a killed tmux
server and a loopback llmock provider), `lane_retire_revive`, `tui_sigint_e2e`,
and 109 REVIVABLE rows on the live store.
