# Harness boundaries and consolidation

The registered transcript/control adapters are Claude, Codex, Kimi and OpenCode.
`Harness`, `Door`, `LiveSessions` and `LaneChannel` remain the boundaries. ACPX
is a named ACP queue transport; its Gemini selector has no transcript adapter.

The initial rg candidate scan returned 219 lines, including tests and static
schema data (`1_harness-baseline-rg.txt`). The final AST inventory is committed as
`5_harness_inventory.json`; its current source receipt is `260_harness-inventory.json`.
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
| `identity::resolve` and `resolve_with` both wrap the same environment rung; caller features use route names as conversation IDs | `resolve_as` owns caller identity; `Identity::conversation` owns route-to-native binding | Removed both vestigial wrappers and registry arguments; mail/lane callers migrated; favorites and concatmap resolve the native conversation; legacy direct IDs remain supported |
| Wrapper writes its cached whole route while registration changes parent/goal | `bus::update_native_route` updates native-owned fields and returns current registration metadata | Five wrapper writes migrated to one atomic update; no parent rollback or resurrection after route deletion; deterministic failure 219 and live parent proof 225 |
| Config model/effort/variant projection helpers and test-only spawn precedence beside CLI precedence | `config::resolve_spawn_preset` returns the complete preset and is used by lane creation | Removed three projection helpers and the model-only selector; tests/concatmap migrated; legacy strings, effort suffixes and executable/variant fields preserved |
| CASS sweep uses unbounded child capture beside existing bounded spawn capture | `worktree::run_captured_with_deadline` | Sweep reuses the process-group deadline implementation; native source markers match complete filename IDs; regression 232 failed and 233 passed |
| OpenCode `prompt_async` merges peer input into an active generation while Boop already owns a pending mailbox drain | `OpencodeDoor::deliver` holds a busy target; existing wrapper drain retries at idle | No new queue or transport; regression 239 failed and 240 passed; authenticated 241/245 preserve the initiating answer and deliver the peer |

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

| File | Symbol | Role | Count |
| --- | --- | --- | ---: |
| `crates/boop-acp/src/channel/1_acpx.rs` | `tests::coordinator_mail_uses_the_persistent_queue` | macro-name-literal | 1 |
| `crates/boop-acp/src/channel/1_acpx.rs` | `tests::foreground_prompt_waits_for_the_same_session` | macro-name-literal | 1 |
| `crates/boop-acp/src/channel/1_acpx.rs` | `tests::route` | enum-reference | 1 |
| `crates/boop-acp/src/channel/acp.rs` | `tests::every_roster_row_spawns_something_in_acp_mode` | macro-name-literal | 1 |
| `crates/boop-acp/src/channel/claude.rs` | `tests::fake_claude` | name-literal | 1 |
| `crates/boop-harness/src/door.rs` | `tests::probe` | enum-reference | 1 |
| `crates/boop-harness/src/door.rs` | `tests::the_default_door_reports_itself_unreachable` | enum-reference | 1 |
| `crates/boop-harness/src/door/claude.rs` | `tests::a_registry_file_becomes_a_live_session` | macro-enum-reference | 2 |
| `crates/boop-harness/src/door/claude.rs` | `tui_launch_tests::a_bare_launch_still_leaves_the_session_to_opened_session` | name-literal | 1 |
| `crates/boop-harness/src/door/claude.rs` | `tui_launch_tests::tui_launch_carries_the_resumed_session_into_the_plan` | name-literal | 1 |
| `crates/boop-harness/src/door/codex.rs` | `tests::a_session_without_a_socket_is_unreachable` | enum-reference | 2 |
| `crates/boop-harness/src/door/codex.rs` | `tests::automatic_resume_uses_observed_settings_and_retains_inline_mode` | name-literal | 1 |
| `crates/boop-harness/src/door/codex.rs` | `tests::the_thread_table_lists_live_threads_only` | macro-enum-reference | 2 |
| `crates/boop-harness/src/door/kimi.rs` | `tests::kimi_reports_no_door` | enum-reference | 1 |
| `crates/boop-harness/src/door/opencode.rs` | `tests::native_model_is_owned_backend_configuration` | name-literal | 1 |
| `crates/boop-harness/src/door/opencode.rs` | `tests::route_uses_its_observed_http_server` | macro-name-literal | 1 |
| `crates/boop-harness/src/door/opencode.rs` | `tests::the_session_list_becomes_live_sessions` | macro-enum-reference | 2 |
| `crates/boop-harness/src/harness.rs` | `supervisor_command_tests::spec` | enum-reference | 1 |
| `crates/boop-harness/src/harness.rs` | `tui_composer_tests::claude_finds_the_prompt_between_the_last_border_pair` | macro-enum-reference | 1 |
| `crates/boop-harness/src/harness.rs` | `tui_composer_tests::codex_finds_a_dynamic_multiline_composer_above_its_footer` | macro-enum-reference | 1 |
| `crates/boop-harness/src/harness.rs` | `tui_composer_tests::kimi_and_opencode_report_their_bottom_bordered_composers` | macro-enum-reference | 2 |
| `crates/boop-harness/src/harness/claude.rs` | `tests::claude_agent_worktrees_lists_locked_and_unlocked_agents` | variant-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `tests::claude_capabilities_are_measured` | variant-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `tests::claude_fixture_projects_through_the_graph_query` | variant-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `tests::claude_launch_resumes_with_session_id` | macro-enum-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `tests::claude_spawn_returns_handle_and_stop_tears_down` | variant-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `tests::extracts_file_paths_and_urls_from_tool_use` | variant-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `tests::session_for` | enum-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `tests::skips_an_invalid_json_line_but_keeps_the_rest` | variant-reference | 1 |
| `crates/boop-harness/src/harness/claude.rs` | `tests::spec` | enum-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::a_partial_trailing_line_is_not_consumed` | variant-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::an_unprojected_kind_warns_once_per_process` | macro-name-literal | 3 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::codex_capabilities_are_measured` | variant-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::codex_fixture_projects_through_the_graph_query` | variant-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::codex_fixture_tool_and_assistant_turns_keep_their_content` | variant-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::codex_spawn_returns_handle_and_stop_tears_down` | variant-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::extracts_patch_apply_paths` | variant-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::known_transcript_uses_persisted_metadata_without_parsing_its_first_record` | enum-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::native_child_observer_projects_the_codex_child_fixture` | variant-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::native_settings_follow_last_observed_turn` | macro-enum-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::project_one_line` | variant-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::reads_a_message_and_a_token_count_line` | variant-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::same_turn_token_counts_sum_into_one_usage_row` | variant-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::session_for` | enum-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::skips_an_invalid_json_line_but_keeps_the_rest` | variant-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::spawn_spec` | enum-reference | 1 |
| `crates/boop-harness/src/harness/codex.rs` | `tests::turn_context_cwd_projects_two_distinct_turn_cwds` | variant-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `tests::a_partial_trailing_line_is_not_consumed` | variant-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `tests::kimi_fixture_projects_through_the_graph_query` | variant-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `tests::kimi_spawns_and_resumes_like_every_other_harness` | variant-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `tests::project_one_line` | variant-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `tests::reads_a_user_message_and_a_tool_call` | variant-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `tests::same_turn_usage_records_sum_into_one_usage_row` | variant-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `tests::session_for` | enum-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `tests::skips_an_invalid_json_line_but_keeps_the_rest` | variant-reference | 1 |
| `crates/boop-harness/src/harness/kimi.rs` | `tests::the_probe_fixture_keeps_every_tool_and_assistant_body` | variant-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::a_model_switch_survives_a_partless_errored_assistant_message` | variant-reference | 3 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::every_content_bearing_part_kind_projects_a_body` | variant-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::exact_sync_candidate_refreshes_the_session_rowid_cursor` | variant-reference | 2 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::no_projected_turn_from_the_fixture_has_an_empty_body` | variant-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::one_shot_requires_a_model_before_starting_a_process` | variant-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::opencode_capabilities_match_the_binary` | variant-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::opencode_fixture_acquisition_and_parent_project_through_graph` | variant-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::opencode_projection_preserves_assistant_and_complete_tool_bodies` | variant-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::opencode_spawn_returns_handle_and_stop_tears_down` | variant-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::preview_uses_the_canonical_supervisor_with_model_variant_resume_and_epilogue` | macro-enum-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::spec` | enum-reference | 1 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::steady_candidate_discovery_skips_message_scan_and_batches_projection_upgrades` | variant-reference | 4 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::streamed_assistant_waits_for_completion_and_legacy_empty_turn_repairs` | macro-enum-reference | 2 |
| `crates/boop-harness/src/harness/opencode.rs` | `tests::streamed_assistant_waits_for_completion_and_legacy_empty_turn_repairs` | variant-reference | 3 |
| `crates/boop-harness/src/identity.rs` | `tests::a_stamp_with_no_parent_clears_an_inherited_parent` | name-literal | 1 |
| `crates/boop-harness/src/identity.rs` | `tests::the_child_stamp_never_carries_the_spawners_session_as_its_own` | name-literal | 1 |
| `crates/boop-harness/src/live.rs` | `tests::bound_route_never_selects_another_thread_by_pane` | macro-name-literal | 1 |
| `crates/boop-harness/src/live.rs` | `tests::session` | enum-reference | 1 |
| `crates/boop-harness/src/registry.rs` | `tests::Echo::id` | enum-reference | 2 |
| `crates/boop-harness/src/registry.rs` | `tests::a_model_spelling_names_its_harness` | macro-enum-reference | 10 |
| `crates/boop-harness/src/registry.rs` | `tests::a_swapped_in_impl_drives_the_shared_rails_under_its_variant` | enum-reference | 1 |
| `crates/boop-harness/src/registry.rs` | `tests::a_swapped_in_impl_drives_the_shared_rails_under_its_variant` | macro-enum-reference | 6 |
| `crates/boop-harness/src/registry.rs` | `tests::a_swapped_in_impl_drives_the_shared_rails_under_its_variant` | macro-name-literal | 2 |
| `crates/boop-harness/src/registry.rs` | `tests::every_variant_resolves_in_the_built_in_registry` | enum-reference | 1 |
| `crates/boop-harness/src/registry.rs` | `tests::every_variant_resolves_in_the_built_in_registry` | macro-enum-reference | 1 |
| `crates/boop-harness/src/registry.rs` | `tests::native_invocations_classify_commands_and_preserve_value_tokens` | name-literal | 15 |
| `crates/boop-harness/src/registry.rs` | `tests::spawn_refusals` | enum-reference | 1 |
| `crates/boop-harness/src/transcript_tests.rs` | `a_resume_id_is_the_stem_for_claude_and_the_session_id_elsewhere` | enum-reference | 2 |
| `crates/boop-harness/src/transcript_tests.rs` | `a_resume_id_is_the_stem_for_claude_and_the_session_id_elsewhere` | macro-enum-reference | 2 |
| `crates/boop-harness/src/transcript_tests.rs` | `an_archived_opencode_session_is_not_a_row` | macro-enum-reference | 3 |
| `crates/boop-harness/src/transcript_tests.rs` | `four_harnesses_lower_into_one_session_shape` | enum-reference | 4 |
| `crates/boop-harness/src/transcript_tests.rs` | `kimi_wire_usage_sums_inputs` | enum-reference | 1 |
| `crates/boop-harness/src/transcript_tests.rs` | `kimi_wire_usage_sums_inputs` | variant-reference | 1 |
| `crates/boop-harness/src/transcript_tests.rs` | `live_session` | enum-reference | 1 |
| `crates/boop-harness/src/transcript_tests.rs` | `opencode_tokens_take_max_not_latest` | enum-reference | 1 |
| `crates/boop-harness/src/transcript_tests.rs` | `opencode_tokens_take_max_not_latest` | variant-reference | 1 |
| `crates/boop-harness/src/transcript_tests.rs` | `session_ref` | enum-reference | 1 |
| `crates/boop-harness/src/transcript_tests.rs` | `wire_shapes_pin_the_instant_key_set` | enum-reference | 2 |
| `crates/boop-harness/src/worktree.rs` | `tests::spec` | enum-reference | 1 |
| `crates/boop-harness/tests/bench_grid.rs` | `session_ref` | enum-reference | 1 |
| `crates/boop-mux/src/lib.rs` | `tests::detailed_session_retains_each_pane` | macro-name-literal | 1 |
| `crates/boop-mux/src/lib.rs` | `tests::parses_pane_identity_and_current_location` | macro-name-literal | 1 |
| `crates/boop-proc/src/concatmap.rs` | `tests::FakeHarness::id` | enum-reference | 2 |
| `crates/boop-proc/src/concatmap.rs` | `tests::row` | enum-reference | 1 |
| `crates/boop-proc/src/concatmap.rs` | `tests::session_ref` | enum-reference | 1 |
| `crates/boop-proc/src/config.rs` | `tests::a_preset_carries_its_harness_and_effort_as_fields` | macro-name-literal | 2 |
| `crates/boop-proc/src/config.rs` | `tests::parses_named_provider_model_presets` | macro-name-literal | 1 |
| `crates/boop-proc/src/deliver.rs` | `tests::FakeClaude::capabilities` | variant-reference | 1 |
| `crates/boop-proc/src/deliver.rs` | `tests::FakeClaude::id` | enum-reference | 2 |
| `crates/boop-proc/src/deliver.rs` | `tests::FakeClaudeLive::live_sessions` | macro-enum-reference | 2 |
| `crates/boop-proc/src/deliver.rs` | `tests::a_claude_coordinator_takes_its_row_at_the_door_with_no_hooks_installed` | enum-reference | 1 |
| `crates/boop-proc/src/deliver.rs` | `tests::a_claude_coordinator_takes_its_row_at_the_door_with_no_hooks_installed` | macro-name-literal | 1 |
| `crates/boop-proc/src/deliver.rs` | `tests::a_row_a_door_already_queued_is_never_pushed_again` | enum-reference | 1 |
| `crates/boop-proc/src/deliver.rs` | `tests::root_session` | enum-reference | 1 |
| `crates/boop-proc/src/deliver.rs` | `tests::the_gate_counts_a_pre_fix_door_queue_row` | enum-reference | 1 |
| `crates/boop-proc/src/deliver.rs` | `tests::unbound_route` | enum-reference | 1 |
| `crates/boop-proc/src/lane.rs` | `tests::a_gpt_model_names_the_codex_harness` | macro-enum-reference | 10 |
| `crates/boop-proc/src/lane.rs` | `tests::an_explicit_harness_wins_over_the_model_spelling` | macro-enum-reference | 6 |
| `crates/boop-proc/src/lane.rs` | `tests::an_explicit_harness_wins_over_the_model_spelling` | macro-name-literal | 2 |
| `crates/boop-proc/src/lane.rs` | `tests::plan_family_models_are_banned_from_opencode` | macro-enum-reference | 4 |
| `crates/boop-proc/src/lane.rs` | `tests::plan_family_models_are_banned_from_opencode` | macro-name-literal | 2 |
| `crates/boop-proc/src/lane.rs` | `tests::plan_family_models_are_banned_from_opencode` | name-literal | 2 |
| `crates/boop-proc/src/lane.rs` | `tests::route_of_kind` | enum-reference | 1 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::current_graph_keeps_discovered_native_sessions_with_idle_status` | name-literal | 1 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::focused_runtime_route_seeds_its_native_session_component` | enum-reference | 1 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::focused_runtime_route_seeds_its_native_session_component` | name-literal | 1 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::focused_tmux_shell_serializes_its_rooted_family_without_cwd_inference` | enum-reference | 2 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::focused_tmux_shell_serializes_its_rooted_family_without_cwd_inference` | macro-name-literal | 1 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::focused_tmux_shell_serializes_its_rooted_family_without_cwd_inference` | name-literal | 2 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::graph_json_contains_trace_event_fixture_and_applies_cwd_and_history_filters` | name-literal | 1 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::graph_projects_sessions_edges_and_shells_from_setwise_relations` | name-literal | 1 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::public_graph_projects_a_live_harness_coordinator_without_a_transcript` | enum-reference | 1 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::public_graph_projects_a_live_harness_coordinator_without_a_transcript` | macro-name-literal | 2 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::qualified_identity_preserves_harness_for_distinct_native_rows` | macro-enum-reference | 4 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::qualified_identity_preserves_harness_for_distinct_native_rows` | name-literal | 4 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::scoped_activity_uses_the_latest_turn_or_usage_timestamp` | name-literal | 1 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::scoped_graph_activity_plan_avoids_whole_corpus_aggregates` | name-literal | 1 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::tmux_session_window_and_pane_evidence_select_the_same_shell` | macro-enum-reference | 4 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::unmatched_harness_routes_project_as_shell_nodes` | enum-reference | 1 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::unmatched_harness_routes_project_as_shell_nodes` | macro-name-literal | 1 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::unresolved_live_lane_appears_as_shell_then_merges_into_sessions` | enum-reference | 1 |
| `crates/boop-store/src/_0_session_graph.rs` | `tests::unresolved_live_lane_appears_as_shell_then_merges_into_sessions` | name-literal | 1 |
| `crates/boop-store/src/activity.rs` | `tests::projects_claude_codex_opencode_kimi_resume_replacement_and_shell_lanes` | name-literal | 6 |
| `crates/boop-store/src/activity.rs` | `tests::scoped_trace_counts_seek_selected_sessions_only` | name-literal | 2 |
| `crates/boop-store/src/bus.rs` | `tests::a_newer_registered_route_outranks_the_file` | macro-name-literal | 1 |
| `crates/boop-store/src/bus.rs` | `tests::a_registry_without_the_goal_field_still_loads` | macro-enum-reference | 2 |
| `crates/boop-store/src/bus.rs` | `tests::a_registry_without_the_parent_field_still_loads` | macro-enum-reference | 2 |
| `crates/boop-store/src/harness_id.rs` | `tests::every_variant_round_trips_through_its_short_id` | enum-reference | 1 |
| `crates/boop-store/src/harness_id.rs` | `tests::every_variant_round_trips_through_its_short_id` | macro-enum-reference | 6 |
| `crates/boop-store/src/harness_id.rs` | `tests::serde_is_the_lowercase_short_id` | enum-reference | 1 |
| `crates/boop-store/src/harness_id.rs` | `tests::serde_is_the_lowercase_short_id` | macro-enum-reference | 3 |
| `crates/boop-store/src/ident.rs` | `tests::a_foreign_dict_harness_value_reads_back_without_error` | macro-enum-reference | 1 |
| `crates/boop-store/src/ident.rs` | `tests::a_lane_spawn_keeps_the_goal_the_path_and_the_brief_bytes` | name-literal | 1 |
| `crates/boop-store/src/ident.rs` | `tests::a_second_delivery_of_one_message_appends_a_transition` | enum-reference | 1 |
| `crates/boop-store/src/ident.rs` | `tests::a_second_delivery_of_one_message_appends_a_transition` | macro-name-literal | 1 |
| `crates/boop-store/src/ident.rs` | `tests::a_v13_store_gains_the_door_columns_and_the_delivery_ledger` | enum-reference | 1 |
| `crates/boop-store/src/ident.rs` | `tests::discovery_projects_empty_unchanged_session_and_parent_edge` | enum-reference | 1 |
| `crates/boop-store/src/ident.rs` | `tests::known_sessions_restore_candidate_metadata_and_cursor` | enum-reference | 1 |
| `crates/boop-store/src/ident.rs` | `tests::known_sessions_stays_under_budget` | enum-reference | 1 |
| `crates/boop-store/src/ident.rs` | `tests::pane_occupant_nickname_is_mutable_without_changing_session_identity` | name-literal | 1 |
| `crates/boop-store/src/ident.rs` | `tests::query_cursors_expose_transcript_identity` | macro-name-literal | 1 |
| `crates/boop-store/src/ident.rs` | `tests::session_for` | enum-reference | 1 |
| `crates/boop-store/src/ident.rs` | `tests::v27_claude_fixture_projects_two_distinct_turn_cwds` | enum-reference | 1 |
| `crates/boop-store/src/ident.rs` | `tests::v27_view_falls_back_to_session_cwd_for_null_turn` | name-literal | 1 |
| `crates/boop-store/src/query.rs` | `tests::live_pid_status_row_carries_nonzero_rss` | enum-reference | 1 |
| `crates/boop-store/src/query.rs` | `tests::typed_rows_populate_for_a_real_session` | enum-reference | 1 |
| `crates/boop-store/src/query.rs` | `tests::typed_rows_populate_for_a_real_session` | macro-name-literal | 1 |
| `crates/boop-store/src/runtime.rs` | `tests::add_session` | name-literal | 1 |
| `crates/boop-store/src/runtime.rs` | `tests::route` | enum-reference | 1 |
| `crates/boop-store/src/session.rs` | `tests::copied_session_id_retains_each_path_and_session_lookup_uses_newest` | macro-name-literal | 1 |
| `crates/boop-store/src/session.rs` | `tests::known` | name-literal | 1 |
| `crates/boop-store/src/summary.rs` | `tests::schema_fixture_joins_activity_runtime_mailbox_and_completion` | enum-reference | 1 |
| `crates/boop-store/src/summary.rs` | `tests::schema_fixture_joins_activity_runtime_mailbox_and_completion` | name-literal | 1 |
| `crates/boop-store/src/usage.rs` | `cte_equality::store_with` | enum-reference | 1 |
| `crates/boop-store/src/usage.rs` | `tests::typed_usage_rows_carry_bucket_and_calls` | enum-reference | 1 |
| `crates/boop-store/src/usage.rs` | `tests::typed_usage_rows_carry_bucket_and_calls` | macro-name-literal | 2 |
| `crates/boop-turnvis/tests/golden.rs` | `FIXTURES` | name-literal | 4 |
| `crates/boop/src/cli/control.rs` | `tests::session` | enum-reference | 1 |
| `crates/boop/src/cli/db.rs` | `tests::CodexFixtureHarness::id` | enum-reference | 2 |
| `crates/boop/src/cli/db.rs` | `tests::CodexFixtureHarness::ingest` | variant-reference | 1 |
| `crates/boop/src/cli/db.rs` | `tests::CodexFixtureHarness::observe_native_children` | variant-reference | 1 |
| `crates/boop/src/cli/db.rs` | `tests::CodexFixtureHarness::read_from` | variant-reference | 1 |
| `crates/boop/src/cli/db.rs` | `tests::FakeHarness::id` | enum-reference | 2 |
| `crates/boop/src/cli/db.rs` | `tests::WatchingHarness::id` | enum-reference | 2 |
| `crates/boop/src/cli/db.rs` | `tests::guardian_reviews_do_not_mail_the_parent_but_delegated_workers_still_do` | enum-reference | 1 |
| `crates/boop/src/cli/db.rs` | `tests::guardian_reviews_do_not_mail_the_parent_but_delegated_workers_still_do` | variant-reference | 1 |
| `crates/boop/src/cli/db.rs` | `tests::native_child_session` | enum-reference | 1 |
| `crates/boop/src/cli/db.rs` | `tests::native_parent_routes` | enum-reference | 1 |
| `crates/boop/src/cli/db.rs` | `tests::precreated_codex_parent_delivers_an_appended_child_completion` | enum-reference | 2 |
| `crates/boop/src/cli/db.rs` | `tests::resident_child_poller_delivers_an_appended_completion_without_manual_sync` | enum-reference | 2 |
| `crates/boop/src/cli/db.rs` | `tests::resident_native_poller_projects_appended_parent_assistant_turns` | enum-reference | 2 |
| `crates/boop/src/cli/db.rs` | `tests::session_with_cwd` | enum-reference | 1 |
| `crates/boop/src/cli/debug.rs` | `tests::the_default_preset_reaches_only_its_own_harness` | macro-enum-reference | 4 |
| `crates/boop/src/cli/job.rs` | `tests::a_fork_brief_carries_the_ask_the_quote_and_the_quoted_turns` | macro-name-literal | 1 |
| `crates/boop/src/cli/job.rs` | `tests::dead_reason_names_no_recorded_session_when_tmux_is_absent` | enum-reference | 1 |
| `crates/boop/src/cli/job.rs` | `tests::dispatch_refuses_an_unregistered_harness` | macro-name-literal | 2 |
| `crates/boop/src/cli/job.rs` | `tests::pstree_carries_the_goal` | enum-reference | 1 |
| `crates/boop/src/cli/job.rs` | `tests::registered_route` | enum-reference | 1 |
| `crates/boop/src/cli/job.rs` | `tests::route_only_delete_drops_the_registry_row_without_tmux` | enum-reference | 1 |
| `crates/boop/src/cli/job.rs` | `tests::tmux_route` | enum-reference | 1 |
| `crates/boop/src/cli/me.rs` | `tests::LiveClaude::id` | enum-reference | 2 |
| `crates/boop/src/cli/me.rs` | `tests::OnePane::live_sessions` | macro-enum-reference | 2 |
| `crates/boop/src/cli/me.rs` | `tests::adopt_reads_the_session_from_the_live_registry_and_explicit_id_wins` | name-literal | 2 |
| `crates/boop/src/cli/mod.rs` | `testkit::route_with` | enum-reference | 1 |
| `crates/boop/src/cli/mod.rs` | `tests::route_goal_round_trips` | enum-reference | 1 |
| `crates/boop/src/debug.rs` | `tests::sync_report_exposes_causes_repairs_and_database_growth` | macro-name-literal | 1 |
| `crates/boop/src/main.rs` | `tests::foreground_acp_coordinator_parses_without_a_subcommand` | macro-name-literal | 1 |
| `crates/boop/src/main.rs` | `tests::foreground_acp_coordinator_parses_without_a_subcommand` | name-literal | 1 |
| `crates/boop/src/main.rs` | `tests::lane_create_and_lane_run_both_take_a_bin_override` | name-literal | 1 |
| `crates/boop/src/main.rs` | `tests::shell_wrappers_share_the_tui_path_preserve_arguments_and_propagate_exit` | name-literal | 13 |
| `crates/boop/tests/1_harness_boundaries.rs` | `Inventory::macro_markers` | dispatch:matches-macro | 1 |
| `crates/boop/tests/1_harness_boundaries.rs` | `Inventory::macro_markers` | macro-name-literal | 4 |
| `crates/boop/tests/1_harness_boundaries.rs` | `Inventory::visit_lit_str` | dispatch:matches-macro | 1 |
| `crates/boop/tests/1_harness_boundaries.rs` | `Inventory::visit_lit_str` | macro-name-literal | 4 |
| `crates/boop/tests/2_native_wrapper.rs` | `generated_wrappers_pass_noninteractive_commands_without_routes` | name-literal | 9 |
| `crates/boop/tests/4_lifecycle_gate.rs` | `Claude` | variant-reference | 1 |
| `crates/boop/tests/4_lifecycle_gate.rs` | `Claude::id` | enum-reference | 2 |
| `crates/boop/tests/4_lifecycle_gate.rs` | `Codex` | variant-reference | 2 |
| `crates/boop/tests/4_lifecycle_gate.rs` | `Codex::entry` | name-literal | 1 |
| `crates/boop/tests/4_lifecycle_gate.rs` | `Codex::id` | enum-reference | 2 |
| `crates/boop/tests/4_lifecycle_gate.rs` | `LifecycleHarness::id` | enum-reference | 1 |
| `crates/boop/tests/4_lifecycle_gate.rs` | `OpenCode::entry` | name-literal | 1 |
| `crates/boop/tests/4_lifecycle_gate.rs` | `OpenCode::id` | enum-reference | 2 |
| `crates/boop/tests/4_lifecycle_gate.rs` | `authenticated_matrix` | dispatch:match | 1 |
| `crates/boop/tests/4_lifecycle_gate.rs` | `authenticated_matrix` | name-literal | 4 |
| `crates/boop/tests/4_lifecycle_gate.rs` | `authenticated_matrix` | variant-reference | 3 |
| `crates/boop/tests/4_lifecycle_gate.rs` | `claude_reader_resolves_exact_session` | enum-reference | 1 |
| `crates/boop/tests/4_lifecycle_gate.rs` | `native_registry_process_transition_preserves_trace` | name-literal | 1 |
| `crates/boop/tests/boop_start_warm.rs` | `DryRunFixture::run` | name-literal | 1 |
| `crates/boop/tests/boop_start_warm.rs` | `spawn_spec` | enum-reference | 1 |
| `crates/boop/tests/coordinator_ping.rs` | `hail_to_a_coordinator_with_no_live_session_is_held_for_its_turn_boundary` | macro-name-literal | 1 |
| `crates/boop/tests/coordinator_ping.rs` | `write_coordinator_route` | macro-name-literal | 1 |
| `crates/boop/tests/deliver_door.rs` | `Echo` | enum-reference | 1 |
| `crates/boop/tests/deliver_door.rs` | `Echo::id` | enum-reference | 1 |
| `crates/boop/tests/deliver_door.rs` | `OneSession::live_sessions` | macro-enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `a_door_falls_back_to_the_last_agent_live_row` | enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `a_door_harness_takes_the_body_and_leaves_one_delivery_row` | enum-reference | 1 |
| `crates/boop/tests/deliver_door.rs` | `a_door_harness_takes_the_body_and_leaves_one_delivery_row` | macro-enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `a_door_harness_takes_the_body_and_leaves_one_delivery_row` | macro-name-literal | 2 |
| `crates/boop/tests/deliver_door.rs` | `a_door_harness_with_no_live_session_is_held_and_never_pasted` | enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `a_hail_with_no_route_is_held_in_the_mailbox` | macro-enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `a_keystrokes_harness_falls_to_the_mailbox_rung` | enum-reference | 1 |
| `crates/boop/tests/deliver_door.rs` | `a_keystrokes_harness_falls_to_the_mailbox_rung` | macro-enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `a_keystrokes_harness_falls_to_the_mailbox_rung` | macro-name-literal | 2 |
| `crates/boop/tests/deliver_door.rs` | `a_lane_end_row_takes_the_door_of_a_live_route` | enum-reference | 1 |
| `crates/boop/tests/deliver_door.rs` | `a_lane_end_row_takes_the_door_of_a_live_route` | macro-enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `a_lane_route_with_no_harness_still_lands_at_its_supervisor` | enum-reference | 1 |
| `crates/boop/tests/deliver_door.rs` | `a_lane_route_with_no_harness_still_lands_at_its_supervisor` | macro-enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `a_reply_to_a_beep_still_takes_the_door_of_the_same_route` | enum-reference | 1 |
| `crates/boop/tests/deliver_door.rs` | `a_reply_to_a_beep_still_takes_the_door_of_the_same_route` | macro-enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `a_route_with_a_live_pane_takes_the_paste_rung` | enum-reference | 1 |
| `crates/boop/tests/deliver_door.rs` | `a_route_with_a_live_pane_takes_the_paste_rung` | macro-enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `a_route_with_no_harness_falls_to_the_mailbox_rung` | enum-reference | 1 |
| `crates/boop/tests/deliver_door.rs` | `a_route_with_no_harness_falls_to_the_mailbox_rung` | macro-enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `a_supervisor_row_never_takes_the_door_of_a_live_route` | enum-reference | 1 |
| `crates/boop/tests/deliver_door.rs` | `a_supervisor_row_never_takes_the_door_of_a_live_route` | macro-enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `concurrent_and_later_retries_do_not_resubmit_an_accepted_message` | enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `echo` | enum-reference | 1 |
| `crates/boop/tests/deliver_door.rs` | `every_landing_records_a_transition_past_appended` | enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `every_landing_records_a_transition_past_appended` | macro-enum-reference | 2 |
| `crates/boop/tests/deliver_door.rs` | `route` | enum-reference | 1 |
| `crates/boop/tests/host_chat.rs` | `EchoHarness::id` | enum-reference | 2 |
| `crates/boop/tests/host_chat.rs` | `two_requests_resume_one_resident_conversation` | enum-reference | 1 |
| `crates/boop/tests/inbox_hooks.rs` | `Coordinator::adopt` | macro-name-literal | 1 |
| `crates/boop/tests/inbox_hooks.rs` | `a_lane_patch_installs_no_hooks` | name-literal | 1 |
| `crates/boop/tests/lane_carcass.rs` | `Doa::create` | name-literal | 1 |
| `crates/boop/tests/lane_carcass.rs` | `a_create_with_an_expect_flag_still_writes_it` | name-literal | 1 |
| `crates/boop/tests/lane_debug.rs` | `a_registered_lane_fills_the_route_mail_and_worktree_sections` | macro-name-literal | 1 |
| `crates/boop/tests/lane_retire_revive.rs` | `a_finished_lane_retires_and_a_beep_revives_it_on_the_same_conversation` | name-literal | 1 |
| `crates/boop/tests/lane_wait_exit.rs` | `CreateFixture::codex_caller_command` | name-literal | 1 |
| `crates/boop/tests/lane_wait_exit.rs` | `CreateFixture::command` | name-literal | 1 |
| `crates/boop/tests/lane_wait_exit.rs` | `a_fresh_codex_acp_session_receives_the_lane_brief` | name-literal | 1 |
| `crates/boop/tests/preset_dry_run.rs` | `the_banned_preset_is_refused_by_name` | macro-name-literal | 1 |
| `crates/boop/tests/registry_kinds.rs` | `coordinator_registration_binds_an_explicit_thread_and_retains_it_on_update` | macro-name-literal | 1 |
| `crates/boop/tests/registry_kinds.rs` | `coordinator_registration_binds_an_explicit_thread_and_retains_it_on_update` | name-literal | 1 |
| `crates/boop/tests/registry_kinds.rs` | `patch_accepts_a_pane_and_preserves_the_registered_route` | macro-name-literal | 1 |
| `crates/boop/tests/session_mood.rs` | `me_favorite_follows_the_callers_bound_native_thread` | enum-reference | 1 |

## Adapter gates and live availability

`100_harness-boundary-suite.log`: 169 passed, one ignored.
`101_acp-boundary-suite.log`: 50 passed, six ignored.
`95_proc-boundary-suite.log`: 159 passed.
`102_cli-boundary-unit.log`: 103 passed.
`103_cli-boundary-integration.log`: 120 passed.

All five installed executable entries (`codex`, `claude`, `ccz`, `kimi`,
`opencode`) returned successful help at this checkpoint, recorded in
`88_available-harnesses.json`. Authenticated wrapper receipt/resume/compact/clear proof now exists for Codex,
Claude and ccz, with per-scenario outcomes in report 2. OpenCode GLM-4.7 has
authenticated idle/busy nonce receipt, but failed other response assertions.
Kimi live behavior remains unverified. Claude settings controls unexpectedly
persisted user defaults in trial 201; restoration and isolated settings execution
remain open, and further Claude settings trials are disabled.
