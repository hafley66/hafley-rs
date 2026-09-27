---
created: 2026-08-19
updated: 2026-09-27
type: feature
status: fixed
priority: high
epic: boop-process
size: M
blocked_by: ['@boop-crate-split']
---

# boop job / boop mail / boop me: the job-control verb surface

## Description

## Description
The verb surface of `docs/design/boop-process.md` section 2: `boop job create|list|get|wait [<job>...]|kill|signal|rm|attach|pane`, `boop mail send|recv|wait`, `boop me whoami|mood|favorite|register`; `db`, `debug`, and `config` stay available. The old `host chat` row is retired because its command and resident-host implementation are absent from current source. Old root spellings become hidden aliases for one release. New mechanisms: `wait` with no args = all my live children; `kill` keeps the row, `rm` forgets; `signal <sig> --children` fans out through parent edges; `attach` = tmux attach to the job's pane; `create --timeout <s>` per job overriding the 300s stall constant.
## Acceptance Criteria
- [x] every active row of the section-2 table has its verb; `boop --help` lists exactly `job mail me db debug config help`.
- [x] old root spellings `beep`, `wait`, `inbox`, and `whoami` are hidden aliases that print one deprecation line and work.
- [x] wait-all with two children (one fails, rc propagates).
- [x] `lane kill` keeps the route and result history; `lane rm` aliases route deletion.
- [x] signal --children reaches two live children and skips a dead one.
- [x] attach on a pane-less job is a named error.
- [x] --timeout kills at N+poll.
- [x] `docs/design/boop-process.md` section 2 and the CLI-facing `crates/boop/docs/*.md` examples use the canonical names.

## Reproduction on installed boop 0.0.10 (248dfdd3)

`boop job --help` returned `unrecognized subcommand 'job'` before this change.

## Implementation receipt

Bare `boop wait` now waits for every registered lane child of the caller, within
one timeout budget, and exits with the first nonzero child rc. `boop wait
--me` retains inbox behavior. `boop job kill <lane>` stops its tmux session
and retains its route; `boop job rm <lane>` removes the route.

## Current implementation receipt

`boop job` dispatches lane create/list/get/wait/kill/rm/attach/pane directly;
`boop mail` exposes send/recv/wait, and `boop me` exposes whoami/register
alongside mood/favorite. `job signal --children` signals live child panes and
prints skip reasons for dead routes. `job create --timeout N` carries a timeout
to the supervisor, which writes rc 124 and closes the channel within one poll
after N. `job rm` forgets a route without stopping its pane.

Tests: `job_commands_are_available_without_the_legacy_beep_lane_prefix`,
`root_help_lists_only_canonical_namespaces_and_help`,
`mail_and_me_commands_have_direct_root_namespaces`,
`signal_children_reaches_live_children_and_skips_a_dead_route`,
`attach_on_a_pane_less_job_reports_the_job_name`,
`runtime_timeout_writes_rc_124_within_one_poll_of_the_deadline`,
`rm_forgets_the_route_and_leaves_a_live_pane_running`, and
`env_pairs_ride_the_dry_run_cmd_line`.

`host chat` was listed as unchanged in the older target table, but is absent
from this source: `boop host chat --help` returns `unrecognized subcommand
'host'`. The resident host implementation was removed with the archived ACP
host experiment, so that table row is obsolete and excluded from the active
CLI contract.

Receipt: `root_help_lists_only_canonical_namespaces_and_help` and
`every_help_example_parses_through_clap` pass with this commit.

Test: `cargo nextest run -p boop -j 2 -E
'test(bare_wait_joins_all_child_lanes_and_propagates_a_failure)'` passes.
`cargo nextest run -p boop -j 2 -E
'test(kill_stops_the_session_and_retains_the_route_for_inspection)'` passes.
