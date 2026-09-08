# Harness boundaries and consolidation

The registered transcript/control adapters are Claude, Codex, Kimi and OpenCode.
`Harness`, `Door`, `LiveSessions` and `LaneChannel` remain the boundaries. ACPX
is a named ACP queue transport; its Gemini selector has no transcript adapter.

The initial rg candidate scan returned 219 lines, including tests and static
schema data (`1_harness-baseline-rg.txt`). The final AST inventory is committed as
`5_harness_inventory.json`; its source receipt is `106_harness-ast-inventory.json`.
Counts below are occurrences by role. A branch and the enum references inside it
are separate observations, so these columns must not be summed as branch counts.
Embedded raw strings and non-Rust sources were also searched with rg;
`89_all-harness-literals.txt` contains 123 Rust candidate lines and two MJS lab
invocations (static executable arguments in `labs/0_agent_control_buy/0_probe.mjs`).

## Behavioral boundary changes

| Competing or misplaced path | Canonical owner and migrated consumers | Removal / compatibility |
| --- | --- | --- |
| `cli/job::run_lane_list` compares Claude, then parses Claude worktree names | `Harness::native_worktrees`; Claude owns porcelain acquisition/parsing; lane list calls the selected adapter | Three Claude-only helper functions and their real-git regression moved out of CLI; `native-claude` output preserved |
| `HarnessId::for_model` and compiled prefix table in the store | `Registry::for_model` asks `Harness::matches_model`; each adapter owns its accepted spelling | Lane/config consumers migrated; model regression moved to registry; store enum retains serialization only |
| `harness_by_id` falls back after an unknown name, alongside two strict lookup wrappers | `Registry::resolve` handles explicit names and the existing omitted-name default | All three CLI lookup helpers removed; partial registry `by_name` now returns None for absent known variants |
| Claude hook settings/protocol interpreted by generic inbox code, even for another harness | `door/0_claude_hooks.rs` and `ClaudeDoor::inbox_hook_installed`; shared caller uses its registered door | Inbox keeps compatibility re-exports and generic drain/ledger operations; a route without a declared harness can still be identified by an installed adapter hook |
| `cli/acpx` contains transport roster, command/model formatting and queue operations | `boop-acp/channel/1_acpx.rs` | CLI retains preset resolution and foreground route orchestration; raw agent selector is retained as `acpx-agent=...` so Gemini works without a `HarnessId` variant |
| Unwired `CodexChannel` and its `RpcChild` beside active ACP lane channel | `Codex::open_channel` -> `AcpChannel`; wrapped live TUI uses `CodexDoor` | Removed two files (538 lines), with zero production consumers. External Rust users of these old public modules must migrate to `Harness::open_channel`; no CLI command was removed |
| Test-only Claude/OpenCode `launch_command` implementations differ from the actual supervisor | `Harness::preview_command` -> `supervisor_command` | Removed both helpers and OpenCode quoting helper; resume/model/variant/epilogue checks now exercise canonical preview; missing-model check exercises the actual one-shot entry |
| Fan-out separately discovers a pane, gates a door and writes acceptance | `boop-proc::deliver_hail_budgeted` | Removed `cli/mail::deliver_through_door`; fan-out's door leg shares routing, budget and acknowledgement. Hook/supervisor presentation remains compatible |
| Codex guardian parent inferred by closest timestamp in a cwd | Explicit native parent field only | Removed time/cwd parent guess. Unparented guardian remains a child with unknown parent |
| Native Codex global notifications select unrelated threads | Codex adapter observes selected responses on the real TUI connection | Start/resume/fork correlation, settings, closure and transport interpretation stay in the adapter; CLI consumes typed events |

## Enforced guard

`tests/1_harness_boundaries.rs::behavioral_harness_dispatch_stays_in_adapters`
parses workspace Rust with `syn`, retaining file, symbol, line, role and test
classification. It rejects concrete harness match arms, if-let patterns, equality
comparisons, `matches!` and string-method dispatch outside the adapter boundary.
The guard's own fixture verifies enum/string/macro cases and test separation.
The only production store exception is `HarnessId::as_str`, the serialization
mapping. The adapter directories and the Claude/ACPX transport modules are the
explicit implementation boundary. Static enum construction, format/schema data,
ACP command roster arrays and test fixtures are inventoried separately.

The guard first failed on the ACPX roster branch (`91_harness-guard.log`), then
passed after its move (`92_harness-guard-after.log`). Current whole-target run:
`103_cli-boundary-integration.log`, 120 passed. It does not claim to understand
arbitrary generated code or every macro language; the literal inventory and rg
receipts cover the reviewed static cases.

## Production occurrence inventory

| File | Symbol | Role | Count |
| --- | --- | --- | ---: |
| `crates/boop-acp/src/channel/1_acpx.rs` | `recognizes_agent` | dispatch:matches-macro | 1 |
| `crates/boop-acp/src/channel/1_acpx.rs` | `recognizes_agent` | macro-name-literal | 4 |
| `crates/boop-acp/src/channel/1_acpx.rs` | `route_agent` | enum-reference | 1 |
| `crates/boop-acp/src/channel/acp.rs` | `KIMI_ADAPTER` | name-literal | 1 |
| `crates/boop-acp/src/channel/acp.rs` | `OPENCODE_ADAPTER` | name-literal | 1 |
| `crates/boop-acp/src/channel/claude.rs` | `ClaudeChannel::open` | name-literal | 1 |
| `crates/boop-harness/src/door/claude.rs` | `RegistryFile::into_live` | enum-reference | 1 |
| `crates/boop-harness/src/door/codex.rs` | `CodexDoor::live_session_for_route` | enum-reference | 1 |
| `crates/boop-harness/src/door/codex.rs` | `CodexDoor::live_sessions` | enum-reference | 1 |
| `crates/boop-harness/src/door/codex.rs` | `queue_message` | name-literal | 1 |
| `crates/boop-harness/src/door/opencode.rs` | `OpencodeDoor::live_sessions` | enum-reference | 1 |
| `crates/boop-harness/src/harness.rs` | `Harness::id` | enum-reference | 1 |
| `crates/boop-harness/src/harness.rs` | `TuiComposer::input_region` | dispatch:match | 1 |
| `crates/boop-harness/src/harness.rs` | `TuiComposer::input_region` | variant-reference | 4 |
| `crates/boop-harness/src/harness/claude.rs` | `Claude` | variant-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `Claude::describe` | enum-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `Claude::id` | enum-reference | 2 |
| `crates/boop-harness/src/harness/claude.rs` | `Claude::matches_model` | name-literal | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `Claude::session_by_id` | enum-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `Claude::spawn` | enum-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `Claude::tui_composer` | variant-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `read_claude` | enum-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `sessions_in_with_known` | enum-reference | 2 |
| `crates/boop-harness/src/harness/codex.rs` | `Codex` | variant-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `Codex::describe` | enum-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `Codex::id` | enum-reference | 2 |
| `crates/boop-harness/src/harness/codex.rs` | `Codex::matches_model` | name-literal | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `Codex::session_by_id` | enum-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `Codex::spawn` | enum-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `Codex::tui_composer` | variant-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `project_line` | name-literal | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `read_codex` | enum-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `sessions_in_with_known` | enum-reference | 2 |
| `crates/boop-harness/src/harness/kimi.rs` | `Kimi` | variant-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `Kimi::describe` | enum-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `Kimi::id` | enum-reference | 2 |
| `crates/boop-harness/src/harness/kimi.rs` | `Kimi::matches_model` | dispatch:string-method | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `Kimi::matches_model` | name-literal | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `Kimi::session_by_id` | enum-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `Kimi::spawn` | enum-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `Kimi::tui_composer` | variant-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `read_kimi` | enum-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `sessions_in` | enum-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `Opencode` | variant-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `Opencode::describe` | enum-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `Opencode::id` | enum-reference | 2 |
| `crates/boop-harness/src/harness/opencode.rs` | `Opencode::one_shot` | name-literal | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `Opencode::read_from` | enum-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `Opencode::spawn` | enum-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `Opencode::tui_composer` | variant-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `opencode_db_path` | name-literal | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `read_opencode` | enum-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `session_from` | enum-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `sessions_from` | enum-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `sync_candidates_from_connection` | enum-reference | 2 |
| `crates/boop-harness/src/harness/opencode.rs` | `write_part` | name-literal | 1 |
| `crates/boop-harness/src/live.rs` | `LiveSession` | enum-reference | 1 |
| `crates/boop-harness/src/registry.rs` | `Registry::by_name` | enum-reference | 1 |
| `crates/boop-harness/src/registry.rs` | `Registry::describe_all` | enum-reference | 1 |
| `crates/boop-harness/src/registry.rs` | `Registry::discover` | macro-enum-reference | 4 |
| `crates/boop-harness/src/registry.rs` | `Registry::for_model` | enum-reference | 1 |
| `crates/boop-harness/src/registry.rs` | `Registry::get` | enum-reference | 1 |
| `crates/boop-harness/src/registry.rs` | `Registry::messages_by_id` | enum-reference | 1 |
| `crates/boop-harness/src/registry.rs` | `Registry::session_ids_for_cwd` | enum-reference | 1 |
| `crates/boop-harness/src/registry.rs` | `Registry::sessions_in_cwd` | enum-reference | 1 |
| `crates/boop-harness/src/transcript.rs` | `Message` | enum-reference | 1 |
| `crates/boop-harness/src/transcript.rs` | `SessionMeta` | enum-reference | 1 |
| `crates/boop-proc/src/deliver.rs` | `Landing::record` | enum-reference | 1 |
| `crates/boop-proc/src/deliver.rs` | `live_session` | enum-reference | 1 |
| `crates/boop-proc/src/deliver.rs` | `projected` | enum-reference | 1 |
| `crates/boop-proc/src/lane.rs` | `DEFAULT_SPAWN_HARNESS` | enum-reference | 2 |
| `crates/boop-proc/src/lane.rs` | `PLAN_FAMILY_TO_HARNESS` | name-literal | 11 |
| `crates/boop-proc/src/lane.rs` | `harness_for_model` | enum-reference | 2 |
| `crates/boop-proc/src/lane.rs` | `harness_for_preset` | enum-reference | 2 |
| `crates/boop-proc/src/lane.rs` | `harness_for_spawn` | enum-reference | 2 |
| `crates/boop-proc/src/lane.rs` | `preset_spawn_check` | enum-reference | 2 |
| `crates/boop-store/src/bus.rs` | `Route` | enum-reference | 1 |
| `crates/boop-store/src/bus.rs` | `route_from_value` | enum-reference | 1 |
| `crates/boop-store/src/bus.rs` | `routes_in` | enum-reference | 1 |
| `crates/boop-store/src/harness_id.rs` | `HarnessId` | enum-reference | 8 |
| `crates/boop-store/src/harness_id.rs` | `HarnessId::as_str` | dispatch:match | 1 |
| `crates/boop-store/src/harness_id.rs` | `HarnessId::as_str` | enum-reference | 4 |
| `crates/boop-store/src/harness_id.rs` | `HarnessId::as_str` | name-literal | 4 |
| `crates/boop-store/src/harness_id.rs` | `HarnessId::from_str` | enum-reference | 2 |
| `crates/boop-store/src/harness_id.rs` | `HarnessId::from_str` | macro-enum-reference | 2 |
| `crates/boop-store/src/harness_id.rs` | `HarnessId::parse` | enum-reference | 2 |
| `crates/boop-store/src/ident.rs` | `Store::append_delivery_transition` | enum-reference | 1 |
| `crates/boop-store/src/ident.rs` | `Store::record_delivery` | enum-reference | 1 |
| `crates/boop-store/src/runtime.rs` | `ResolvedRoute` | enum-reference | 1 |
| `crates/boop-store/src/session.rs` | `SessionRef` | enum-reference | 1 |
| `crates/boop-store/src/session.rs` | `SpawnSpec` | enum-reference | 1 |
| `crates/boop/labs/0_tmux_process_env/probe.rs` | `main` | name-literal | 2 |
| `crates/boop/src/cli/acpx.rs` | `run_foreground` | enum-reference | 1 |
| `crates/boop/src/cli/db.rs` | `AdapterPhase` | enum-reference | 1 |
| `crates/boop/src/cli/db.rs` | `AdapterPhase::new` | enum-reference | 1 |
| `crates/boop/src/cli/debug.rs` | `default_preset_for_harness` | enum-reference | 1 |
| `crates/boop/src/cli/debug.rs` | `run_lane_debug` | macro-enum-reference | 1 |
| `crates/boop/src/cli/job.rs` | `run_agent` | enum-reference | 1 |
| `crates/boop/src/cli/job.rs` | `run_lane` | enum-reference | 1 |
| `crates/boop/src/cli/job.rs` | `run_lane_list` | enum-reference | 1 |
| `crates/boop/src/cli/job.rs` | `run_lane_list` | macro-enum-reference | 2 |
| `crates/boop/src/cli/job.rs` | `run_resolve` | enum-reference | 1 |
| `crates/boop/src/cli/mail.rs` | `run_list` | enum-reference | 1 |
| `crates/boop/src/cli/me.rs` | `register_route` | enum-reference | 1 |
| `crates/boop/src/cli/paste.rs` | `run_paste` | enum-reference | 2 |

## Test occurrence inventory

| File | Role | Count |
| --- | --- | ---: |
| `crates/boop-acp/src/channel/1_acpx.rs` | enum-reference | 1 |
| `crates/boop-acp/src/channel/1_acpx.rs` | macro-name-literal | 2 |
| `crates/boop-acp/src/channel/acp.rs` | macro-name-literal | 1 |
| `crates/boop-acp/src/channel/claude.rs` | name-literal | 1 |
| `crates/boop-harness/src/door.rs` | enum-reference | 2 |
| `crates/boop-harness/src/door/claude.rs` | macro-enum-reference | 2 |
| `crates/boop-harness/src/door/claude.rs` | name-literal | 2 |
| `crates/boop-harness/src/door/codex.rs` | enum-reference | 2 |
| `crates/boop-harness/src/door/codex.rs` | macro-enum-reference | 2 |
| `crates/boop-harness/src/door/kimi.rs` | enum-reference | 1 |
| `crates/boop-harness/src/door/opencode.rs` | macro-enum-reference | 2 |
| `crates/boop-harness/src/harness.rs` | enum-reference | 1 |
| `crates/boop-harness/src/harness.rs` | macro-enum-reference | 4 |
| `crates/boop-harness/src/harness/claude.rs` | enum-reference | 2 |
| `crates/boop-harness/src/harness/claude.rs` | macro-enum-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | variant-reference | 6 |
| `crates/boop-harness/src/harness/codex.rs` | enum-reference | 3 |
| `crates/boop-harness/src/harness/codex.rs` | macro-name-literal | 3 |
| `crates/boop-harness/src/harness/codex.rs` | variant-reference | 12 |
| `crates/boop-harness/src/harness/kimi.rs` | enum-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | variant-reference | 8 |
| `crates/boop-harness/src/harness/opencode.rs` | enum-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | macro-enum-reference | 3 |
| `crates/boop-harness/src/harness/opencode.rs` | variant-reference | 19 |
| `crates/boop-harness/src/identity.rs` | name-literal | 2 |
| `crates/boop-harness/src/live.rs` | enum-reference | 1 |
| `crates/boop-harness/src/registry.rs` | enum-reference | 5 |
| `crates/boop-harness/src/registry.rs` | macro-enum-reference | 17 |
| `crates/boop-harness/src/registry.rs` | macro-name-literal | 2 |
| `crates/boop-harness/src/transcript_tests.rs` | enum-reference | 12 |
| `crates/boop-harness/src/transcript_tests.rs` | macro-enum-reference | 5 |
| `crates/boop-harness/src/transcript_tests.rs` | variant-reference | 2 |
| `crates/boop-harness/src/worktree.rs` | enum-reference | 1 |
| `crates/boop-harness/tests/bench_grid.rs` | enum-reference | 1 |
| `crates/boop-mux/src/lib.rs` | macro-name-literal | 2 |
| `crates/boop-proc/src/concatmap.rs` | enum-reference | 4 |
| `crates/boop-proc/src/config.rs` | macro-name-literal | 3 |
| `crates/boop-proc/src/deliver.rs` | enum-reference | 7 |
| `crates/boop-proc/src/deliver.rs` | macro-enum-reference | 2 |
| `crates/boop-proc/src/deliver.rs` | macro-name-literal | 1 |
| `crates/boop-proc/src/deliver.rs` | variant-reference | 1 |
| `crates/boop-proc/src/lane.rs` | enum-reference | 1 |
| `crates/boop-proc/src/lane.rs` | macro-enum-reference | 20 |
| `crates/boop-proc/src/lane.rs` | macro-name-literal | 4 |
| `crates/boop-proc/src/lane.rs` | name-literal | 2 |
| `crates/boop-store/src/_0_session_graph.rs` | enum-reference | 6 |
| `crates/boop-store/src/_0_session_graph.rs` | macro-enum-reference | 8 |
| `crates/boop-store/src/_0_session_graph.rs` | macro-name-literal | 4 |
| `crates/boop-store/src/_0_session_graph.rs` | name-literal | 13 |
| `crates/boop-store/src/activity.rs` | name-literal | 8 |
| `crates/boop-store/src/bus.rs` | macro-enum-reference | 4 |
| `crates/boop-store/src/bus.rs` | macro-name-literal | 1 |
| `crates/boop-store/src/harness_id.rs` | enum-reference | 2 |
| `crates/boop-store/src/harness_id.rs` | macro-enum-reference | 9 |
| `crates/boop-store/src/ident.rs` | enum-reference | 7 |
| `crates/boop-store/src/ident.rs` | macro-enum-reference | 1 |
| `crates/boop-store/src/ident.rs` | macro-name-literal | 2 |
| `crates/boop-store/src/ident.rs` | name-literal | 3 |
| `crates/boop-store/src/query.rs` | enum-reference | 2 |
| `crates/boop-store/src/query.rs` | macro-name-literal | 1 |
| `crates/boop-store/src/runtime.rs` | enum-reference | 1 |
| `crates/boop-store/src/runtime.rs` | name-literal | 1 |
| `crates/boop-store/src/session.rs` | macro-name-literal | 1 |
| `crates/boop-store/src/session.rs` | name-literal | 1 |
| `crates/boop-store/src/summary.rs` | enum-reference | 1 |
| `crates/boop-store/src/summary.rs` | name-literal | 1 |
| `crates/boop-store/src/usage.rs` | enum-reference | 2 |
| `crates/boop-store/src/usage.rs` | macro-name-literal | 2 |
| `crates/boop-turnvis/tests/golden.rs` | name-literal | 4 |
| `crates/boop/src/cli/control.rs` | enum-reference | 1 |
| `crates/boop/src/cli/db.rs` | enum-reference | 16 |
| `crates/boop/src/cli/db.rs` | variant-reference | 4 |
| `crates/boop/src/cli/debug.rs` | macro-enum-reference | 4 |
| `crates/boop/src/cli/job.rs` | enum-reference | 5 |
| `crates/boop/src/cli/job.rs` | macro-name-literal | 3 |
| `crates/boop/src/cli/me.rs` | enum-reference | 2 |
| `crates/boop/src/cli/me.rs` | macro-enum-reference | 2 |
| `crates/boop/src/cli/me.rs` | name-literal | 2 |
| `crates/boop/src/cli/mod.rs` | enum-reference | 2 |
| `crates/boop/src/debug.rs` | macro-name-literal | 1 |
| `crates/boop/src/main.rs` | macro-name-literal | 1 |
| `crates/boop/src/main.rs` | name-literal | 15 |
| `crates/boop/tests/1_harness_boundaries.rs` | dispatch:matches-macro | 2 |
| `crates/boop/tests/1_harness_boundaries.rs` | macro-name-literal | 8 |
| `crates/boop/tests/boop_start_warm.rs` | enum-reference | 1 |
| `crates/boop/tests/boop_start_warm.rs` | name-literal | 1 |
| `crates/boop/tests/coordinator_ping.rs` | macro-name-literal | 2 |
| `crates/boop/tests/deliver_door.rs` | enum-reference | 18 |
| `crates/boop/tests/deliver_door.rs` | macro-enum-reference | 22 |
| `crates/boop/tests/deliver_door.rs` | macro-name-literal | 4 |
| `crates/boop/tests/host_chat.rs` | enum-reference | 3 |
| `crates/boop/tests/inbox_hooks.rs` | macro-name-literal | 1 |
| `crates/boop/tests/inbox_hooks.rs` | name-literal | 1 |
| `crates/boop/tests/lane_carcass.rs` | name-literal | 2 |
| `crates/boop/tests/lane_debug.rs` | macro-name-literal | 1 |
| `crates/boop/tests/lane_retire_revive.rs` | name-literal | 1 |
| `crates/boop/tests/lane_wait_exit.rs` | name-literal | 3 |
| `crates/boop/tests/preset_dry_run.rs` | macro-name-literal | 1 |
| `crates/boop/tests/registry_kinds.rs` | macro-name-literal | 2 |
| `crates/boop/tests/registry_kinds.rs` | name-literal | 1 |

## Adapter gates and live availability

`100_harness-boundary-suite.log`: 169 passed, one ignored.
`101_acp-boundary-suite.log`: 50 passed, six ignored.
`95_proc-boundary-suite.log`: 159 passed.
`102_cli-boundary-unit.log`: 103 passed.
`103_cli-boundary-integration.log`: 120 passed.

All five installed executable entries (`codex`, `claude`, `ccz`, `kimi`,
`opencode`) returned successful help at this checkpoint, recorded in
`88_available-harnesses.json`. Authenticated live proof exists for Codex only.
Other harness authentication/live interaction has not been exercised; executable
availability alone is not a live lifecycle pass.
