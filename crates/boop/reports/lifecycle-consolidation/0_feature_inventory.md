# Boop feature inventory

## Executable regression entry point

`CARGO_TARGET_DIR=<dedicated-target> just boop-check deterministic` runs the seven
Boop package suites, `boop --features dl6` tests, and no-default-features check.
The complete command most recently passed in `254_central-deterministic.log`.
The earlier no-default import cleanup passed in `172_no-default-clean.log`. CI calls this
recipe. The previous `scripts/door-e2e.sh` now delegates to the central live mode.

`BOOP_E2E_ROOT=<task-owned-directory> just boop-check live` invokes all four
required entries. `tests/4_lifecycle_gate.rs::LifecycleHarness` shares scenarios
and assertions; Codex, Claude and OpenCode implement native operations, with ccz
as a second Claude configuration. It returns failure for any FAIL or BLOCKED
scenario. Default deterministic runs explicitly ignore authenticated tests;
passing deterministic coverage does not assert live provider coverage.

Feature gates now follow dependencies: `dl6` enables `agent-read`; the library
host/concatmap modules require that reader feature. Delivery receipts remain in
the core store with the compatible `query::DeliveryRow` export. The missing dl6
fixture field and no-default compile errors were reproduced before correction.

Observed native sessions now use `bind_native_session` for trace/PID/pane binding,
including same-process clear transitions identified by exact PID. Claude exact
session lookup and observed model metadata are adapter-owned. OpenCode uses its
route's HTTP server for delivery and idle observation, owns newly launched
backends, and preserves the configured model without provider-default fallback.
Its explicit model override now reaches owned backend configuration; native
GLM-4.7 execution was observed. Remaining native controls still need review.

Current-help corrections: hook help names route registration instead of removed
`adopt`; `docs/tell.md` distinguishes removed command aliases from the retained
hidden `--body` argument and labels its old ACP diagram as a historical design.

Wrapper checkpoint: generated Bash functions pass help, version, CLI subcommands
and print-mode calls directly to the native executable. Adapter-owned argument
classification preserves values that resemble commands. The failing actual-wrapper
fixture (`107_wrapper-passthrough-before.log`) returned exit 1 instead of 23 for all
six calls; after the fix all arguments/exit codes match and no route is registered
(`108_wrapper-passthrough-after.log`). `boop tui --name` supports a stable name with
a database-scoped lifetime lock and refuses a live owner or lane supervisor.
Automatic Codex resume applies observed model/effort and retains inline mode.

Delivery checkpoint: direct sends, ACPX queues, all reachable child fan-out legs
and resident held-mail retries share `deliver_hail_budgeted`. Route admission
uses an explicitly released file lock; the prior-acceptance lookup runs inside
that lock. Transport receipt and mailbox ack have one owner. Held native-child
completions retain their durable outbox entry until transport acceptance or
observed native transcript notification.

Work in progress, based on `66cbe8e`. Worktree: `refactor/boop-lifecycle-consolidation`.
Installed help and isolated reproduction receipts live under
`/private/tmp/boop-lifecycle-consolidation-proof-01a08191`.

| Surface | Observed path | Regression / status |
| --- | --- | --- |
| Register coordinator/native | `main.rs::AgentCmd` → `cli/job.rs::run_agent` → `cli/me.rs::register_route` | Session/pane options added; merge preserves omitted metadata; 8 registry regressions pass |
| Attach existing pane | `beep lane patch` → `cli/me.rs::register_route` | Compatibility entry shares registration; `%pane` works; missing target errors; existing kind preserved |
| Delivery | `beep` → `cli/mail.rs::deliver_hail` → `boop-proc::deliver::land` | Lane routes short circuit to supervisor; incident reproduced |
| Shell wrapper | `shell-init bash` → `boop_wrap` → `boop tui` → `run_native_tui` | Codex/Claude/ccz/Kimi/OpenCode share the path inside and outside tmux; forwarding and exit regression passes |
| Interactive lifecycle | `run_native_tui` → `Door::tui_launch` → adapter observation → existing route/session store | Actual Codex start/resume/settings/clear responses observed; raw live receipts in report 2 |

The observed CLI/help/test matrix and canonical paths follow below.
Recursive current help capture includes 101 pages (see `200_current-help/manifest.json`).

Reader/config overrides: `BOOP_READER_HOME` controls only Boop's transcript and
session registry reads. `BOOP_CONFIG` selects the Boop config file. Neither changes
the native executable's HOME/CODEX_HOME. Codex reader and door now share one home
resolver and respect existing CODEX_HOME outside an offline reader fixture.
Legacy NDJSON import retains in-place tail compatibility, but files containing
no envelopes acquire no write transaction. CLI integration: 118 passed.

## Registration consolidation

`beep agent register` and compatibility `beep lane patch` now call
`cli/me.rs::register_route`. Explicit `--session-id` and `--tmux` are supported
by registration. Supplied fields merge under the existing store transaction;
omitted fields and route kind survive. A new pane attachment is a coordinator.
Existing lane ownership survives patching. Invalid targets return an error.

Removed `run_adopt`, `run_adopt_with`, duplicate pane discovery/fallback and
unreachable adoption hook-removal branch. Hook management remains `inbox hooks`.
Removed the CLI copy of route serialization. Callers use
`boop-store::bus::route_to_value`; legacy registry field spellings remain a
compatibility obligation. Ordinary `write_route` uses the single-row store
upsert instead of rewriting the complete registry through CAS.

Initial incident regressions: 3 failures before changes; `registry_kinds` after
changes: 8 passed, 0 failed. Registration helper unit tests: 3 passed;
coordinator compatibility tests: 4 passed.

## Native launch consolidation

The generated shell functions previously had separate tmux and direct-executable
branches. Both now use `boop tui`; default route names use pane or process identity
so simultaneous launches in one cwd do not overwrite each other. Parent linkage
is retained on process resume. All five entries, including the `ccz` executable
alias, have forwarding and exit-code coverage. Bash is the supported shell and
the user's configured variant; authenticated trials used `/opt/homebrew/bin/bash`.

Codex previously used a shared daemon and cwd/time discovery. Its adapter now
owns a private backend and observes the real TUI connection. Actual selected
thread responses bind the route; subscribed settings update model and effort.
`LiveSessions::live_session_for_route` owns endpoint interpretation, and the
delivery code delegates to it. Other adapters retain their existing deterministic
contracts. Their discovery paths and remaining competing liveness writes are
listed in report 3 for the next consolidation chunk.

## Current default-build command matrix

The worktree executable returned exit 0 for all 101 recursive help pages in
`200_current-help/manifest.json`. The following 72 leaf commands are the observed
default-build surface. Test references identify owning suites, not a claim that
every flag combination or authenticated transition passed. Raw help preserves
every observed argument, default, description and parser constraint.

| Command | Canonical implementation | Test ownership / limitation |
| --- | --- | --- |
| `boop shell-init` | `harness::shell_init` → generated Bash wrapper | `2_native_wrapper.rs`; authenticated `4_lifecycle_gate.rs` |
| `boop tui` | `cli/control.rs::run_native_tui` → `Door::tui_launch` | control units, `2_native_wrapper.rs`, `4_lifecycle_gate.rs` |
| `boop debug` | `cli/debug.rs::run_debug` → telemetry store | `lane_debug.rs` |
| `boop whoami` | `cli/me.rs::run_whoami` → env/explicit identity | harness identity tests |
| `boop wait` | `cli/job.rs::run_wait` → `mailwait` | `wait_mail.rs`, `lane_wait_exit.rs` |
| `boop beep paste` | `cli/paste.rs::run_paste` → adapter paste keys / tmux | paste units; live paste not exercised |
| `boop beep ps` | `cli/job.rs::run_ps` → process snapshot | job process units |
| `boop beep pstree` | `cli/job.rs::run_pstree` → process tree | job process units |
| `boop db agent-summary` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db search` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db sessions` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db lanes` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db mail` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db schema` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db status` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop agent summary` | `cli/db.rs::run_public_agent_command` → summary/session graph | `cli/db.rs` public-agent parser/query tests |
| `boop agent sessions` | `cli/db.rs::run_public_agent_command` → summary/session graph | `cli/db.rs` public-agent parser/query tests |
| `boop inbox drain` | `cli/mail.rs` → adapter hooks / mailbox drain | `inbox_hooks.rs` |
| `boop inbox hooks` | `cli/mail.rs` → adapter hooks / mailbox drain | `inbox_hooks.rs` |
| `boop me mood` | `cli/me.rs::run_me_mood` → session attributes | `session_mood.rs` |
| `boop me favorite` | `cli/me.rs::run_me_favorite` → assistant turn selection / favorite store | parser units; route-to-native-thread resolution remains an identified gap |
| `boop tag add` | `cli/tag.rs::run_tag_*` → shared `agent_tag` table | store tag tests; per-command authenticated execution not required |
| `boop tag recent` | `cli/tag.rs::run_tag_*` → shared `agent_tag` table | store tag tests; per-command authenticated execution not required |
| `boop tag search` | `cli/tag.rs::run_tag_*` → shared `agent_tag` table | store tag tests; per-command authenticated execution not required |
| `boop tag list` | `cli/tag.rs::run_tag_*` → shared `agent_tag` table | store tag tests; per-command authenticated execution not required |
| `boop tag of` | `cli/tag.rs::run_tag_*` → shared `agent_tag` table | store tag tests; per-command authenticated execution not required |
| `boop tag sources` | `cli/tag.rs::run_tag_*` → shared `agent_tag` table | store tag tests; per-command authenticated execution not required |
| `boop tag rm` | `cli/tag.rs::run_tag_*` → shared `agent_tag` table | store tag tests; per-command authenticated execution not required |
| `boop tag backfill` | `cli/tag.rs::run_tag_*` → shared `agent_tag` table | store tag tests; per-command authenticated execution not required |
| `boop config path` | `cli/debug.rs::run_config` → `boop-proc::config` | `presets_json.rs`, `preset_dry_run.rs` |
| `boop config show` | `cli/debug.rs::run_config` → `boop-proc::config` | `presets_json.rs`, `preset_dry_run.rs` |
| `boop config presets` | `cli/debug.rs::run_config` → `boop-proc::config` | `presets_json.rs`, `preset_dry_run.rs` |
| `boop beep harness list` | `cli/job.rs::run_beep` → registry adapters | harness adapter contracts; `1_harness_boundaries.rs` |
| `boop beep harness get` | `cli/job.rs::run_beep` → registry adapters | harness adapter contracts; `1_harness_boundaries.rs` |
| `boop beep lane list` | `cli/job.rs::run_beep_lane` → lane/store/multiplexer | `registry_kinds.rs`, `lane_create_env.rs`, `lane_carcass.rs`, `lane_retire_revive.rs`, `lane_wait_exit.rs`; individual operations vary |
| `boop beep lane create` | `cli/job.rs::run_beep_lane` → lane/store/multiplexer | `registry_kinds.rs`, `lane_create_env.rs`, `lane_carcass.rs`, `lane_retire_revive.rs`, `lane_wait_exit.rs`; individual operations vary |
| `boop beep lane get` | `cli/job.rs::run_beep_lane` → lane/store/multiplexer | `registry_kinds.rs`, `lane_create_env.rs`, `lane_carcass.rs`, `lane_retire_revive.rs`, `lane_wait_exit.rs`; individual operations vary |
| `boop beep lane where` | `cli/job.rs::run_beep_lane` → lane/store/multiplexer | `registry_kinds.rs`, `lane_create_env.rs`, `lane_carcass.rs`, `lane_retire_revive.rs`, `lane_wait_exit.rs`; individual operations vary |
| `boop beep lane patch` | `cli/job.rs::run_beep_lane` → lane/store/multiplexer | `registry_kinds.rs`, `lane_create_env.rs`, `lane_carcass.rs`, `lane_retire_revive.rs`, `lane_wait_exit.rs`; individual operations vary |
| `boop beep lane delete` | `cli/job.rs::run_beep_lane` → lane/store/multiplexer | `registry_kinds.rs`, `lane_create_env.rs`, `lane_carcass.rs`, `lane_retire_revive.rs`, `lane_wait_exit.rs`; individual operations vary |
| `boop beep lane prune` | `cli/job.rs::run_beep_lane` → lane/store/multiplexer | `registry_kinds.rs`, `lane_create_env.rs`, `lane_carcass.rs`, `lane_retire_revive.rs`, `lane_wait_exit.rs`; individual operations vary |
| `boop beep lane route` | `cli/job.rs::run_beep_lane` → lane/store/multiplexer | `registry_kinds.rs`, `lane_create_env.rs`, `lane_carcass.rs`, `lane_retire_revive.rs`, `lane_wait_exit.rs`; individual operations vary |
| `boop beep lane pane` | `cli/job.rs::run_beep_lane` → lane/store/multiplexer | `registry_kinds.rs`, `lane_create_env.rs`, `lane_carcass.rs`, `lane_retire_revive.rs`, `lane_wait_exit.rs`; individual operations vary |
| `boop beep agent register` | `cli/job.rs::run_agent` → route registration/completion | `registry_kinds.rs`, `native_agent_liveness.rs`, `lane_completion_row.rs` |
| `boop beep agent done` | `cli/job.rs::run_agent` → route registration/completion | `registry_kinds.rs`, `native_agent_liveness.rs`, `lane_completion_row.rs` |
| `boop beep message ack` | `cli/job.rs::run_beep` → mailbox acknowledgment | `tell.rs`, mailbox store contracts |
| `boop beep fork join` | `cli/job.rs::run_fork_join` / `run_fork_diff` → git worktree | `cli/job.rs` and harness worktree unit tests |
| `boop beep fork diff` | `cli/job.rs::run_fork_join` / `run_fork_diff` → git worktree | `cli/job.rs` and harness worktree unit tests |
| `boop db session list` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db session get` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db turn list` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db turn get` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db chat list` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db touch list` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db command list` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db fetch list` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db skill list` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db pr list` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db span list` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db edge list` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db usage blocks` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db usage burn-rate` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db price list` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db price set` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db favorite add` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db favorite list` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db favorite show` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db favorite edit` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db favorite delete` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db sync create` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop db sync-cursor list` | `cli/db.rs::run_db` → `boop-store::query` / typed store methods | `cli/db.rs` unit contracts; store query tests; sync rows additionally `sync_discovery.rs`, `sync_convoy.rs` |
| `boop beep lane message list` | `cli/job.rs::run_beep_lane` → lane/store/multiplexer | `registry_kinds.rs`, `lane_create_env.rs`, `lane_carcass.rs`, `lane_retire_revive.rs`, `lane_wait_exit.rs`; individual operations vary |

The send form `boop beep ROUTE BODY` and SQL form `boop db SQL` coexist with
their command groups and therefore are not leaf help nodes. They dispatch to
`cli/mail.rs::run_send` and `cli/db.rs::run_passthrough_at`, respectively.
`boop --preset NAME --name ROUTE` selects the foreground ACP coordinator path
in `cli/acpx.rs`. These are additional callable forms, not omitted leaves.

### Compatibility and feature-gated forms

- Hidden `beep --body` retains old scripts; positional BODY plus `--body` now
  conflicts before insertion (`189` fail before, `190` 21 tell tests pass after).
- `whoami --from` aliases `--as` in the same Clap field.
- Hidden lane-create `--harness`, `--model`, `--wait` remain compatibility flags;
  presets and the shared wait implementation own their behavior. Public
  `--reclaim` is documented as a compatibility no-op.
- Hidden `beep lane run` is the supervisor process entry, not another creation
  implementation. The lane creation path emits it.
- Feature `dl6` exposes `host` and hidden `concatmap`. Default help cannot
  enumerate these. Their contracts run in the central dl6 gate, including
  `host_chat.rs` and `concatmap_e2e.rs`.
- `agent summary` and `db agent-summary` both call `run_agent_summary`; the
  additional public spelling remains a compatibility/consolidation obligation.
  `agent sessions` produces the runtime session graph; `db session list` and
  `db sessions` read different typed/query views. These require field-level
  comparison before any supported view is removed.

### Identity, persistence and failures

Configuration ownership is `boop-proc/src/config.rs`. `BOOP_CONFIG` overrides
the platform config path. `loaded()` caches its first result; explicit `load(path)`
reads that path, missing files return the empty config, and parse/read failures
remain errors. Fields are `default-model-preset`, `model-presets`, `model-harness`
and `opencode-banned`. Each preset carries harness/model/effort/variant/bin;
legacy string presets and `model@effort` resolve through `resolve_preset` and
`ModelPreset::split_effort`. An explicit effort field wins over the suffix.
`presets_json.rs` snapshots table output and checks JSON field/order equivalence;
`preset_dry_run.rs` exercises launch validation without spawning providers.

Lane creation resolves an explicit model or preset, determines its harness,
then applies a default only when `default_preset_for_harness` finds the same
owner. Variant and binary CLI arguments override the preset fields. The old
test-only `resolve_spawn_model` has been replaced by `resolve_spawn_preset`,
which returns every preset field and is called by lane creation. Precedence tests
now exercise that production function. The model/effort/variant projection
wrappers were removed; concatmap and tests read the resolved preset fields.
Preset contracts and CLI integration passed in `250` and `251`.

Native thread IDs are owned by adapters. `bind_native_session` owns observed
route/thread, process and trace updates; settings flow through `NativeTuiEvent`.
A bound route now resolves its thread before considering any pane, and stale
bound routes return no target (`183` failed, `184` passed). Pane numbers are
server-local, so pane-only discovery remains a compatibility risk for unbound
routes. Supervisor routes retain their separate lane execution owner.

Mail envelopes, delivery acceptance and acknowledgments have distinct meanings:
store insertion persists intent; acceptance records the transport outcome;
native transcript plus intended TUI display proves receipt in live tests.
`deliver_hail_budgeted` owns admission/retries. An acceptance-before-durable-write
crash window remains unverified. SQLite/WAL contention and isolated reader/config
roots are exercised by `0_sqlite_contention.rs`, `sync_convoy.rs`,
`native_projector_contention.rs` and `temp_home_rail.rs`.

Telemetry uses the existing tracing/observe pipeline and debug queries. Source
inspection includes process snapshots, worktree/base-SHA tracking, tags/favorites,
price/usage queries, PR/fetch/skill/span facts, native subagent edges and completion
outboxes. The argument/option inventory below records every captured option;
individual option combinations without a named regression remain unverified.

### Issue-facing integrations inspected

`boop-store/src/summary.rs::AgentSummary` is schema version 1 and exposes Boop
runtime/activity facts. Its contract excludes CASS issue, reservation and provider
records. Both CLI summary spellings use this contract. PR facts are transcript
observations in `agent_pr`/`dict_pr`, queried by `db prs`; this path does not create
or modify GitHub issues or pull requests. Store tests exercise PR deduplication.

`cli/job.rs::run_sweep` calls `cass_hit`, which executes `cass search ID --robot
--limit 20` and scopes source paths through `scoped_to_agent`. Missing executable,
nonzero status and malformed JSON all return no hit. Age expiry and
`--close-routeless` append acknowledgments without transcript proof. These are
mail maintenance outcomes. The external command now reuses
`worktree::run_captured_with_deadline` with a 20-second process-group deadline.
Native marker source paths (`native-session=ID`) resolve by a complete filename
ID suffix, rejecting partial IDs and directory-only matches. The regression
failed in `232_sweep-scope-before.log` and passed in `233_sweep-scope-after.log`.
No production sweep was executed. Shared-database transcript paths still require
a separate session identifier from CASS before they can prove a scoped hit.
Live E2E receipt assertions use native adapter observations and
do not depend on this maintenance integration.

## Observed argument and option inventory

Captured from all 101 help pages in `200_current-help/manifest.json`. The command
handlers and tests are mapped above; help-only presence does not prove every
option combination. `--help` is common to these surfaces, and root `--version`
prints the build identity. Hidden compatibility options are listed separately
above. Feature-gated `dl6` forms are covered by the central feature gate.

| Command | Usage arguments | Options beyond help/version |
| --- | --- | --- |
| `boop` | `boop [OPTIONS] [COMMAND]` | `--preset <PRESET>`, `--name <NAME>`, `--mail-dir <MAIL_DIR>` |
| `boop agent` | `boop agent <COMMAND>` | none |
| `boop agent sessions` | `boop agent sessions [OPTIONS]` | `--cwd <CWD>`, `--history`, `--tmux <TMUX>`, `--history-since-ts <HISTORY_SINCE_TS>`, `--format <FORMAT>`, `--mail-dir <MAIL_DIR>` |
| `boop agent summary` | `boop agent summary [OPTIONS]` | `--format <FORMAT>`, `--mail-dir <MAIL_DIR>` |
| `boop beep` | `boop beep [OPTIONS] [ROUTE] [BODY]` | `--as <NAME>`, `--kind <KIND>`, `--timeout <TIMEOUT>`, `--no-wait`, `--mail-dir <MAIL_DIR>` |
| `boop beep agent` | `boop beep agent <COMMAND>` | none |
| `boop beep agent done` | `boop beep agent done [OPTIONS] <NAME>` | `--rc <RC>`, `--mail-dir <MAIL_DIR>` |
| `boop beep agent register` | `boop beep agent register [OPTIONS] <NAME>` | `--kind <KIND>`, `--parent <PARENT>`, `--on-parent-death <ON_PARENT_DEATH>`, `--harness <HARNESS>`, `--session-id <SESSION_ID>`, `--tmux <TMUX>`, `--cwd <CWD>`, `--worktree <WORKTREE>`, `--mail-dir <MAIL_DIR>` |
| `boop beep fork` | `boop beep fork [OPTIONS] [COMMENT]` | `--preset <PRESET>`, `--cwd <CWD>`, `--parent <PARENT>`, `--dry-run`, `--mail-dir <MAIL_DIR>` |
| `boop beep fork diff` | `boop beep fork diff [OPTIONS] <COMMENT>` | `--lane <LANE>`, `--stat`, `--mail-dir <MAIL_DIR>` |
| `boop beep fork join` | `boop beep fork join [OPTIONS] <COMMENT>` | `--lane <LANE>`, `--no-merge`, `--no-reply`, `--dry-run`, `--mail-dir <MAIL_DIR>` |
| `boop beep harness` | `boop beep harness <COMMAND>` | none |
| `boop beep harness get` | `boop beep harness get <HARNESS>` | none |
| `boop beep harness list` | `boop beep harness list` | none |
| `boop beep lane` | `boop beep lane <COMMAND>` | none |
| `boop beep lane create` | `boop beep lane create [OPTIONS]` | `--branch <BRANCH>`, `--brief <BRIEF>`, `--goal <GOAL>`, `--mood <MOOD>`, `--trace <TRACE>`, `--no-start`, `--cwd <CWD>`, `--base-sha <BASE_SHA>`, `--expect-path <EXPECT_PATH>`, `--expect-commit-subject <EXPECT_COMMIT_SUBJECT>`, `--expect-commits-at-least <EXPECT_COMMITS_AT_LEAST>`, `--env <KEY=VAL>`, `--parent <PARENT>`, `--on-parent-death <ON_PARENT_DEATH>`, `--preset <PRESET>`, `--variant <VARIANT>`, `--bin <BIN>`, `--wait-timeout <WAIT_TIMEOUT>`, `--lane <LANE>`, `--tmux <TMUX>`, `--socket <SOCKET>`, `--mail-dir <MAIL_DIR>`, `--dry-run`, `--reclaim` |
| `boop beep lane delete` | `boop beep lane delete [OPTIONS] [LANE]` | `--route-only`, `--state <STATE>`, `--dry-run`, `--mail-dir <MAIL_DIR>` |
| `boop beep lane get` | `boop beep lane get [OPTIONS] <LANE>` | `--touched`, `--mail-dir <MAIL_DIR>` |
| `boop beep lane list` | `boop beep lane list [OPTIONS]` | `--state <STATE>`, `--harness <HARNESS>`, `--all`, `--mail-dir <MAIL_DIR>` |
| `boop beep lane message` | `boop beep lane message <COMMAND>` | none |
| `boop beep lane message list` | `boop beep lane message list [OPTIONS] <LANE>` | `--mail-dir <MAIL_DIR>` |
| `boop beep lane pane` | `boop beep lane pane [OPTIONS] <LANE>` | `--lines <LINES>`, `--socket <SOCKET>`, `--mail-dir <MAIL_DIR>` |
| `boop beep lane patch` | `boop beep lane patch [OPTIONS] --tmux <TMUX> <LANE>` | `--tmux <TMUX>`, `--harness <HARNESS>`, `--session-id <SESSION_ID>`, `--cwd <CWD>`, `--model <MODEL>`, `--mode <MODE>`, `--parent <PARENT>`, `--goal <GOAL>`, `--mail-dir <MAIL_DIR>` |
| `boop beep lane prune` | `boop beep lane prune [OPTIONS]` | `--dry-run`, `--mail-dir <MAIL_DIR>` |
| `boop beep lane route` | `boop beep lane route [OPTIONS] <LANE>` | `--mail-dir <MAIL_DIR>` |
| `boop beep lane where` | `boop beep lane where [OPTIONS] <LANE>` | `--mail-dir <MAIL_DIR>` |
| `boop beep message` | `boop beep message <COMMAND>` | none |
| `boop beep message ack` | `boop beep message ack [OPTIONS]` | `--lane <LANE>`, `--box <BOX>`, `--close-routeless`, `--max-age-days <MAX_AGE_DAYS>`, `--mail-dir <MAIL_DIR>` |
| `boop beep paste` | `boop beep paste [OPTIONS] <PATH>` | `--route <ROUTE>`, `--pane <PANE>`, `--harness <HARNESS>`, `--as-path`, `--mail-dir <MAIL_DIR>` |
| `boop beep ps` | `boop beep ps [OPTIONS] [LANE]` | `--all`, `--mail-dir <MAIL_DIR>` |
| `boop beep pstree` | `boop beep pstree [OPTIONS]` | `--all`, `--format <FORMAT>`, `--mail-dir <MAIL_DIR>` |
| `boop config` | `boop config <COMMAND>` | none |
| `boop config path` | `boop config path` | none |
| `boop config presets` | `boop config presets [OPTIONS]` | `--format <FORMAT>` |
| `boop config show` | `boop config show` | none |
| `boop db` | `boop db [OPTIONS] [SQL]` | `--format <FORMAT>` |
| `boop db agent-summary` | `boop db agent-summary [OPTIONS]` | `--format <FORMAT>`, `--mail-dir <MAIL_DIR>` |
| `boop db chat` | `boop db chat <COMMAND>` | none |
| `boop db chat list` | `boop db chat list [OPTIONS]` | `--harness <HARNESS>`, `--session <SESSION>`, `--role <ROLE>`, `--since <SINCE>`, `--until <UNTIL>`, `--turn-from <TURN_FROM>`, `--turn-to <TURN_TO>`, `--path <PATH>`, `--limit <LIMIT>`, `--format <FORMAT>`, `--all`, `--follow` |
| `boop db command` | `boop db command <COMMAND>` | none |
| `boop db command list` | `boop db command list [OPTIONS]` | `--session <SESSION>`, `--since <SINCE>`, `--until <UNTIL>`, `--like <LIKE>`, `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db edge` | `boop db edge <COMMAND>` | none |
| `boop db edge list` | `boop db edge list [OPTIONS]` | `--session <SESSION>`, `--limit <LIMIT>` |
| `boop db favorite` | `boop db favorite <COMMAND>` | none |
| `boop db favorite add` | `boop db favorite add [OPTIONS]` | `--file <FILE>`, `--note <NOTE>`, `--source <SOURCE>` |
| `boop db favorite delete` | `boop db favorite delete <ID>` | none |
| `boop db favorite edit` | `boop db favorite edit [OPTIONS] <ID>` | `--note <NOTE>`, `--source <SOURCE>` |
| `boop db favorite list` | `boop db favorite list [OPTIONS]` | `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db favorite show` | `boop db favorite show [OPTIONS] <ID>` | `--format <FORMAT>` |
| `boop db fetch` | `boop db fetch <COMMAND>` | none |
| `boop db fetch list` | `boop db fetch list [OPTIONS]` | `--session <SESSION>`, `--since <SINCE>`, `--until <UNTIL>`, `--like <LIKE>`, `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db lanes` | `boop db lanes [OPTIONS]` | `--days <DAYS>`, `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db mail` | `boop db mail [OPTIONS] <ROUTE>` | `--kind <KIND>`, `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db pr` | `boop db pr <COMMAND>` | none |
| `boop db pr list` | `boop db pr list [OPTIONS]` | `--session <SESSION>`, `--since <SINCE>`, `--until <UNTIL>`, `--like <LIKE>`, `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db price` | `boop db price <COMMAND>` | none |
| `boop db price list` | `boop db price list` | none |
| `boop db price set` | `boop db price set [OPTIONS] --input-per-mtok <INPUT_PER_MTOK> --output-per-mtok <OUTPUT_PER_MTOK> --cache-write-5m-per-mtok <CACHE_WRITE_5M_PER_MTOK> --cache-write-1h-per-mtok <CACHE_WRITE_1H_PER_MTOK> --cache-read-per-mtok <CACHE_READ_PER_MTOK> <MODEL>` | `--input-per-mtok <INPUT_PER_MTOK>`, `--output-per-mtok <OUTPUT_PER_MTOK>`, `--cache-write-5m-per-mtok <CACHE_WRITE_5M_PER_MTOK>`, `--cache-write-1h-per-mtok <CACHE_WRITE_1H_PER_MTOK>`, `--cache-read-per-mtok <CACHE_READ_PER_MTOK>`, `--source <SOURCE>` |
| `boop db schema` | `boop db schema [OPTIONS]` | `--format <FORMAT>` |
| `boop db search` | `boop db search [OPTIONS] <TEXT>` | `--days <DAYS>`, `--harness <HARNESS>`, `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db session` | `boop db session <COMMAND>` | none |
| `boop db session get` | `boop db session get [OPTIONS] <SESSION>` | `--format <FORMAT>` |
| `boop db session list` | `boop db session list [OPTIONS]` | `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db sessions` | `boop db sessions [OPTIONS]` | `--days <DAYS>`, `--harness <HARNESS>`, `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db skill` | `boop db skill <COMMAND>` | none |
| `boop db skill list` | `boop db skill list [OPTIONS]` | `--session <SESSION>`, `--since <SINCE>`, `--until <UNTIL>`, `--like <LIKE>`, `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db span` | `boop db span <COMMAND>` | none |
| `boop db span list` | `boop db span list [OPTIONS]` | `--session <SESSION>`, `--since <SINCE>`, `--until <UNTIL>`, `--like <LIKE>`, `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db status` | `boop db status [OPTIONS]` | `--window <WINDOW>`, `--format <FORMAT>` |
| `boop db sync` | `boop db sync <COMMAND>` | none |
| `boop db sync create` | `boop db sync create [OPTIONS]` | `--rebuild`, `--forever`, `--mail-dir <MAIL_DIR>` |
| `boop db sync-cursor` | `boop db sync-cursor <COMMAND>` | none |
| `boop db sync-cursor list` | `boop db sync-cursor list [OPTIONS]` | `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db touch` | `boop db touch <COMMAND>` | none |
| `boop db touch list` | `boop db touch list [OPTIONS]` | `--session <SESSION>`, `--since <SINCE>`, `--until <UNTIL>`, `--like <LIKE>`, `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db turn` | `boop db turn <COMMAND>` | none |
| `boop db turn get` | `boop db turn get [OPTIONS] <SESSION> <TURN>` | `--format <FORMAT>` |
| `boop db turn list` | `boop db turn list [OPTIONS]` | `--harness <HARNESS>`, `--session <SESSION>`, `--role <ROLE>`, `--since <SINCE>`, `--until <UNTIL>`, `--turn-from <TURN_FROM>`, `--turn-to <TURN_TO>`, `--path <PATH>`, `--limit <LIMIT>`, `--format <FORMAT>` |
| `boop db usage` | `boop db usage [OPTIONS]` | `--format <FORMAT>`, `--show-sql` |
| `boop db usage blocks` | `boop db usage blocks [OPTIONS]` | `--window-hours <WINDOW_HOURS>`, `--active`, `--format <FORMAT>` |
| `boop db usage burn-rate` | `boop db usage burn-rate [OPTIONS]` | `--window-minutes <WINDOW_MINUTES>`, `--format <FORMAT>` |
| `boop debug` | `boop debug [OPTIONS] [LANE]` | `--since <SINCE>`, `--lane <LANE>`, `--json`, `--mail-dir <MAIL_DIR>` |
| `boop inbox` | `boop inbox <COMMAND>` | none |
| `boop inbox drain` | `boop inbox drain [OPTIONS]` | `--as <NAME>`, `--hook <HOOK>`, `--mail-dir <MAIL_DIR>` |
| `boop inbox hooks` | `boop inbox hooks [OPTIONS] --name <NAME>` | `--name <NAME>`, `--cwd <CWD>`, `--uninstall` |
| `boop me` | `boop me [OPTIONS] <COMMAND>` | `--mail-dir <MAIL_DIR>` |
| `boop me favorite` | `boop me favorite [OPTIONS] [INDEX]` | `--note <NOTE>` |
| `boop me mood` | `boop me mood [OPTIONS] [NAME]` | `--clear`, `--as <SESSION>` |
| `boop shell-init` | `boop shell-init <SHELL>` | none |
| `boop tag` | `boop tag <COMMAND>` | none |
| `boop tag add` | `boop tag add [OPTIONS] <TAG>...` | `--source <SOURCE>` |
| `boop tag backfill` | `boop tag backfill` | none |
| `boop tag list` | `boop tag list [OPTIONS]` | `--format <FORMAT>` |
| `boop tag of` | `boop tag of <SOURCE>` | none |
| `boop tag recent` | `boop tag recent [OPTIONS]` | `-n, --limit <LIMIT>`, `--format <FORMAT>` |
| `boop tag rm` | `boop tag rm --source <SOURCE> <TAG>` | `--source <SOURCE>` |
| `boop tag search` | `boop tag search [OPTIONS] <QUERY>` | `-n, --limit <LIMIT>`, `--format <FORMAT>` |
| `boop tag sources` | `boop tag sources <TAG>` | none |
| `boop tui` | `boop tui [OPTIONS] <HARNESS> [-- <ARGS>...]` | `--bin <EXECUTABLE>`, `--name <NAME>`, `--cwd <CWD>`, `--mail-dir <MAIL_DIR>` |
| `boop wait` | `boop wait [OPTIONS] [ID-OR-LANE]` | `--me`, `--as <NAME>`, `--wait-timeout <WAIT_TIMEOUT>`, `--mail-dir <MAIL_DIR>` |
| `boop whoami` | `boop whoami [OPTIONS]` | `--json`, `--as <NAME>`, `--mail-dir <MAIL_DIR>` |
