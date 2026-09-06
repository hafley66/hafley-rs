---
created: 2026-09-05
updated: 2026-09-05
type: bug
status: open
priority: normal
epic: boop-process
---

# A dead tmux pane keeps a lane name and blocks respawn with duplicate session

## Description

## Description

A lane that exits leaves its tmux session standing with a dead pane. `lane create
--lane <same-name>` then fails with `duplicate session`, so a lane name cannot be
reused inside one tmux server without a manual `tmux kill-session`.

The pane is dead, not live: nothing is attached to it and no process runs in it.
`tmux new-session -s <name>` refuses because the name is taken, and the lane
create path reads that refusal as a spawn failure rather than as a carcass to
clear.

## Receipts

- feature-fork-render, 2026-09-05 19:39. The lane exited; the respawn under the
  same name printed `duplicate session`.
- `crates/boop/tests/lane_carcass.rs` is the nearest existing rail; it covers a
  dead route, not a dead pane holding a live session name.

## Expected

One of, decided when the fix is written:

- `lane create` detects a session whose panes are all dead and reclaims the
  name, saying which carcass it cleared.
- Or `lane create` refuses with the exact `tmux kill-session -t <name>` line to
  run, rather than the bare `duplicate session` text.

Either way the message names the session and the next command; a reader must not
have to compose one.

## Acceptance Criteria

- [ ] Root cause named: which code path creates the session, which one is meant
      to reap it, and why the reap did not run on lane exit.
- [ ] `lane create --lane <name>` after that lane exits either succeeds or exits
      with a message naming the session and the command that clears it.
- [ ] A test drives a fake multiplexer through exit-then-respawn under one lane
      name.
