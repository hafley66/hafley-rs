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

# Claude delivery through the door only; hook inbox is a rung, not a verb group

## Description

## Description

Claude coordinators take mail by unix-socket door or by `.claude/settings.json` hooks (`inbox drain`, `inbox hooks`). Two paths, one verb group that exists for the second.

Cut: hook inbox becomes an internal ladder rung; `inbox` verbs hidden.

## Acceptance Criteria

- [x] `boop --help` has no `inbox`
- [x] claude coordinator e2e receives a row with no hooks installed

## Agent Runs

### 2026-08-25T13:15:26Z · @feat-epic-wave-b

a461b0b the hook inbox stays where it already was, an internal rung of the deliver.rs ladder; the 'inbox' clap group is #[command(hide = true)] so boop --help lists no inbox command (the installed hook still calls 'boop inbox drain'), and the lane doctrine line that called a lane's mailbox its inbox now says mailbox. New receipt a_claude_coordinator_takes_its_row_at_the_door_with_no_hooks_installed: a fake claude door plus a coordinator route whose cwd carries no .claude/settings.json lands at Rung::Door with transitions appended -> accepted-by-harness and no hook rung. Live receipt: --preset zsonnet lane chore-door-claude in the scratch tree-repo (no .claude/settings.json anywhere in the repo or the worktree) took its dispatch row, committed f853c5d..5ee32dd, then took a second row m-b3ba3b79 whose transitions read appended then held-for-turn-boundary (lane supervisor) and ran a turn on it. Suite: boop-proc 121 passed 0 failed; full run over the six crates green. Note: 'inbox' still appears twice in the help's DELIVERY doctrine as the name of the ladder rung and of the queued-in-hook-inbox transition, which are the recorded vocabulary rather than a verb.
