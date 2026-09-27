---
created: 2026-08-19
updated: 2026-09-27
type: feature
status: open
priority: high
epic: boop-process
size: M
blocked_by: ['@boop-crate-split']
---

# boop job / boop mail / boop me: the job-control verb surface

## Description

## Description
The verb surface of `docs/design/boop-process.md` section 2: `boop job create|list|get|wait [<job>...]|kill|signal|rm|attach|pane`, `boop mail send|recv|wait`, `boop me whoami|mood|favorite|register`; `db`, `debug`, `config`, `host chat` unchanged; old spellings become hidden aliases for one release. New mechanisms: `wait` with no args = all my live children; `kill` keeps the row, `rm` forgets (carcass-safe, #35); `signal <sig> --children` fans out through parent edges; `attach` = tmux attach to the job's pane; `create --timeout <s>` per job overriding the 300s stall constant.
## Acceptance Criteria
- [ ] every row of the section-2 table has its verb; `boop --help` lists exactly `job mail me db debug config host help`.
- [ ] each old spelling is a hidden alias that prints one deprecation line to stderr and works.
- [x] wait-all with two children (one fails, rc propagates).
- [x] `lane kill` keeps the route and result history; `lane rm` aliases route deletion.
- [x] signal --children reaches two live children and skips a dead one.
- [x] attach on a pane-less job is a named error.
- [x] --timeout kills at N+poll.
- [ ] `docs/design/boop-process.md` section 2 updated to match; `crates/boop/docs/*.md` verbs renamed.

## Reproduction on installed boop 0.0.10 (248dfdd3)

`boop job --help` returned `unrecognized subcommand 'job'` before this change.

## Implementation receipt

Bare `boop wait` now waits for every registered lane child of the caller, within
one timeout budget, and exits with the first nonzero child rc. `boop wait
--me` retains inbox behavior. `boop beep lane kill <lane>` stops its tmux
session and retains its route; `boop beep lane rm <lane>` removes the route.
The remaining namespace operations stay open.

## Current implementation receipt

`boop job` dispatches lane create/list/get/wait/kill/rm/attach/pane directly;
`boop mail` exposes send/recv/wait, and `boop me` exposes whoami/register
alongside mood/favorite. `job signal --children` signals live child panes and
prints skip reasons for dead routes. `job create --timeout N` carries a timeout
to the supervisor, which writes rc 124 and closes the channel within one poll
after N. `job rm` forgets a route without stopping its pane.

Tests: `job_commands_are_available_without_the_legacy_beep_lane_prefix`,
`mail_and_me_commands_have_direct_root_namespaces`,
`signal_children_reaches_live_children_and_skips_a_dead_route`,
`attach_on_a_pane_less_job_reports_the_job_name`,
`runtime_timeout_writes_rc_124_within_one_poll_of_the_deadline`,
`rm_forgets_the_route_and_leaves_a_live_pane_running`, and
`env_pairs_ride_the_dry_run_cmd_line`.

Test: `cargo nextest run -p boop -j 2 -E
'test(bare_wait_joins_all_child_lanes_and_propagates_a_failure)'` passes.
`cargo nextest run -p boop -j 2 -E
'test(kill_stops_the_session_and_retains_the_route_for_inspection)'` passes.
