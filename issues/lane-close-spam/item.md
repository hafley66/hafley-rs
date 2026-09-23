---
created: 2026-09-23
updated: 2026-09-23
type: bug
status: open
priority: medium
labels: [domain-boop]
size: S
---

# A lane the coordinator stops sends several completion rows, not one

## Description

`boop --help` COMPLETION promises ONE row `lane <id> done rc=<n>` per lane exit. Stopping a
lane from the coordinator side (`tmux kill-session -t <lane>`) produced two result rows for the
same spawn, both hailed into the coordinator's turn:

```
[boop m-c0067d3e from perf-boop-graph-sqlite-only] lane perf-boop-graph-sqlite-only done rc=129
  (killed by SIGHUP; incomplete: no commit with subject '...' head=9bf91bb6 commits_past_base=0)
[boop m-25998c5b from perf-boop-graph-sqlite-only] lane perf-boop-graph-sqlite-only done rc=101
  (panic: failed printing to stdout: Input/output error (os error 5); incomplete: ...)
```

A finished lane also sends `lane <id> retired: idle 60s after the result row` after its result
row (feat-signals-query-visibility, m-50316a7e), a third hail for one lane.

Observed causes:
1. The supervisor writes the rc=129 row on SIGHUP, then its own `println!` to the dead pane
   panics (stdout EIO) and the panic path writes a second result row (rc=101).
2. The `retired` notice is hailed like a result, although the parent already has the result.

Result rows are written in `crates/boop/src/cli/job.rs` (~1800, `body: "lane {name} done rc={rc}"`)
and delivered with `mail::deliver_hail`.

## Expected

- Exactly one result row per spawn, whatever the exit path. A later exit path for the same spawn
  is a no-op (idempotent on lane + spawn id).
- Writes to a closed stdout never panic the supervisor (ignore EPIPE/EIO on its own prints).
- A stop the coordinator initiated (kill-session, `lane delete`, `beep scream`) reports as one row
  with a distinct reason (`stopped by coordinator`), and `--expect-*` assertions are not evaluated
  for it.
- `retired` is recorded in the store but not hailed into the parent's turn.

## Regression tests

- Supervisor test: kill the pane's tmux session under a running lane; the parent mailbox holds
  exactly one `kind=result` row for that spawn.
- Supervisor test: stdout closed before the supervisor prints; no panic, one result row.
- Retire test: after the idle retire, no hail is delivered to the parent (mailbox row may exist
  with a non-hail kind).
