# CHECK producer audit

`18_check_audit.tsv` covers all 22 migrated columns and the five other CHECK columns in the complete schema. A column's observed values do not establish closure. The audit follows writer inputs upstream to their producing matches, enums, or external strings.

## Open text

Four migrated columns have no membership CHECK and bypass the closed-domain validator:

- `agent_touch.raw_verb`: Claude copies `block.name` at `boop-harness/src/harness/claude.rs:1113`; Codex copies the function name at `boop-harness/src/harness/codex.rs:1133`. `emit_tool_fact` forwards the original name to `add_touch`.
- `agent_turn.role`: Codex reads `payload.role` at `boop-harness/src/harness/codex.rs:1427`; Kimi reads `message.role` at `boop-harness/src/harness/kimi.rs:962`; OpenCode reads `data.role` at `boop-harness/src/harness/opencode.rs:1258` and forwards it at line 884. These paths do not restrict roles to a finite match before writing.
- `model_price.source`: `boop/src/main.rs:2098` declares an unrestricted CLI string, forwarded at `boop/src/cli/db.rs:1829`.
- `sync_root_stamp.harness`: repository searches find its historical DDL and migration, with no current producer. The audit cannot prove a closed source, so its text stays unrestricted.

The old shared `dict_verb` mixed normalized verbs and raw tool names. Both columns are decoded before that dictionary is dropped. Open data remains plain text; the open dictionary tables from the original migration remain unchanged.

## Closed producer inventory

The audit table cites one producing site per column. Additional sites completing each finite vocabulary:

- Attachment rules: `boop/src/cli/job.rs:580` (`lane-create`), `boop-proc/src/supervise.rs:2976` (`lane-run`) and line 2983 (`supervisor-conversation`), `boop/src/cli/control.rs:247` (`native-tui-session`), and store projection/backfill literals (`derived-session-relation`, `backfill-spawned-edge`).
- Attribute keys: control writes `process_pid`, `process_start_secs`, `process_previous_session`, and `effort`; the store owns `reset_ts` and `mood`. No caller forwards an external attribute key.
- Edge kinds: the control match at `boop/src/cli/mail.rs:995` admits `retry` and `resume` in addition to the previously admitted control labels. Delivery edges format `deliver-` plus the two-value `Delivery::as_str()` at `boop-proc/src/supervise.rs:3209`. Discovery, native-child completion, and completion receipts use internal literals. Historical labels remain permitted.
- Normalized verbs: the actual store writer uses `ident.rs:4692`, with lowercased `read`, `write`, `edit`, `list`, `glob`, `multiedit`, and `grep`. The older capitalized spellings remain permitted for existing rows. This producer uses a tool-name match, rather than `Access`.
- Observation/relation provenance: store literals `trace-attach`, `trace-event`, `live-status`, `transcript-sync`, and `transcript-session-metadata`; the Claude relation producer adds `claude-transcript`. `legacy-agent-trace-span` remains for historical rows.
- Trace kind: every `TraceRecorder::record` caller in `boop-proc/src/supervise.rs` binds a literal. The audit adds `resource-sample`, `resource-interrupt`, `resource-pause`, `resource-resume`, `parent-death`, and `stale`, already produced by current code. `boop/src/invoke.rs:202` binds the `cli-invocation` constant; control line 265 binds `session-boundary`.
- Trace classification: supervisor calls bind literals or finite if/match results. The audit adds `over-limit`, `within-limit`, `accepted`, `paused`, `resumed`, and `stale`. External error and receipt strings go into `detail`, not classification.
- Trace delivery state: the supervisor uses the `Delivery` enum's two-value match. CLI outcome producers bind `started` in invocation start, `ok` or `error` at `boop/src/main.rs:463`, and the three-way clap match at `boop/src/invoke.rs:131`. No external receipt text is persisted as delivery state.
- Harness: session discovery and lane spawning use `HarnessId`; delivery writers accept `Option<HarnessId>` directly. Observation harness comes from the typed discovered session or None. `gemini` remains as a legacy stored label.
- Status: transcript refresh, explicit TUI control, lane liveness, and lane completion select internal live/idle/detached/dead/closed labels from state branches. `record_status` updates both the current row and span.

The numeric `every_ms > 0` CHECK is a range invariant for a typed integer, rather than a finite text vocabulary. It remains. Boolean selection is restricted to zero and one.

## Regression

`open_harness_values_survive_writes_migration_and_rebuild` writes `MultiEdit` through `write_tool_fact`, a future harness role through `write_turn`, an arbitrary price provider through `price_set`, and a legacy root-stamp harness. It downgrades the fixture to actual dictionary columns at schema 39 and reopens through migration 40, asserting exact preserved text. After rebuild it repeats the raw tool and role writes. The migration rollback test now corrupts normalized fetch kind, which remains closed.
