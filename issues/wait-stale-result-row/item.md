---
created: 2026-08-25
updated: 2026-08-25
type: bug
reporter: claude-5
status: open
priority: high
---

# boop wait <lane> returns the previous turn's done row after a resume

## Description

`boop wait <lane>` after a resume turn returns the result row the previous turn wrote, so the caller reads "done" while the lane is mid-turn.

Receipt (2026-08-25, lane chore-alias-cut): turn 1 ended with a question, supervisor wrote `lane chore-alias-cut done rc=0` (seq 9200). A `boop beep chore-alias-cut <answer>` started a resume turn (seq 9202, accepted-by-harness). `boop wait chore-alias-cut --wait-timeout 3600` returned at once with `WORKTREE-UNTOUCHED chore-alias-cut: no new commits` on seq 9200 while the harness was running tool calls #38-41 and `main.rs` was dirty. Workaround: poll `agent_mail` for `kind='result' and seq > <the beep's seq>`.

`lane_result_rc_since` exists in `crates/boop/src/cli/job.rs`; `run_lane_wait` passes no `since`.

## Acceptance Criteria
- [ ] `boop wait <lane>` ignores result rows older than the lane's latest turn start (route `last turn` or the newest dispatch/request row the supervisor claimed)
- [ ] a test: result row, then a claimed request row, then wait blocks until a newer result row
