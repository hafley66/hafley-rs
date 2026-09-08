# Task state and join instructions

Work in progress, 2026-09-08. Base `66cbe8e`; branch `refactor/boop-lifecycle-consolidation`.
No merge, push or global install. Worktree: `/Users/chrishafley/projects/hafley-rs/.boop-worktrees/refactor/boop-lifecycle-consolidation`.
Dedicated CARGO_TARGET_DIR: `/private/tmp/boop-lifecycle-consolidation-target`.
Raw receipts: `/private/tmp/boop-lifecycle-consolidation-proof-01a08191`.

Audit lane `refactor-boop-lifecycle-consolidation`, pane `%1811`, pane PID `22078`,
remains live. Audit thread `01a08191-263d-7971-b11b-10b44d4bbd09` has observed
model `gpt-6-astra`, effort `max`. Parent: `sprefa-ivm-extract-parent`.
Join outside tmux: `tmux attach-session -t refactor-boop-lifecycle-consolidation`.
Join inside tmux: `tmux switch-client -t refactor-boop-lifecycle-consolidation`.
The CLI has no `beep lane join`. This lane stays open while work remains.

## Commits

- `29fad26 fix(boop): preserve route ownership when binding existing panes`
- `2376ef5 fix(boop-store): isolate trails and coalesce lifecycle observations`
- `b815481 feat(boop): observe wrapped Codex lifecycle through owned connections`
- `501d44c fix(boop): retain process observations and release owned TUI resources`
- `f4480d1 fix(boop): isolate fixture readers and avoid telemetry mailbox writes`
- `8b6097d`: adapter-owned model inference, hook settings, native
  worktree discovery and ACPX transport; removed unused Codex transport modules,
  duplicate dispatch/preview helpers and fan-out door implementation; architectural
  guard and symbol inventory. Migration and compatibility table in report 1.
- `d2cc152`: noninteractive passthrough, stable named ownership,
  observed settings on automatic resume and atomic process detachment. Wrapper
  regression passed (`108_wrapper-passthrough-after.log`), harness 171 passed / 1
  ignored (`109_harness-wrapper-suite.log`), control 11 passed (`110_native-control-suite.log`).
- Current delivery cluster: one route admission lock covers the transport and
  acceptance receipt. Concurrent and later retries made three calls before
  (`111_delivery-retry-before.log`) and one after (`112_delivery-retry-after.log`).
  ACPX and all reachable fan-out legs now use the same ladder, budget and ack.
  Child completions stay pending on a hold (`114_child-held-before.log`); swallowed
  delivery errors no longer mark them delivered. Effort lookup now joins the
  session dictionary (`113_observed-effort-before.log`).
  Fork-inherited descriptors kept locks alive after close; the deterministic
  failure is `118_inherited-lock-before.log`. RouteLock explicitly unlocks now.
  Gates: store 176 passed (`119_store-delivery-suite.log`), process 159 passed
  (`120_proc-delivery-after.log`), CLI integration 122 passed
  (`121_cli-delivery-after.log`), CLI unit 105 passed (`122_cli-unit-after.log`).

Delivery retry limit: prior durable acceptance prevents another transport call.
A process dying after remote acceptance and before its local receipt still leaves
an ambiguous outcome. The current native queue API provides no tested idempotency
key; crash-window exactly-once is not claimed.

## Current gates

All cargo commands use the dedicated target directory above.

| Command | Result | Raw log |
| --- | --- | --- |
| `cargo test -p boop-store --lib` | Earlier checkpoint 176 passed; latest shared changes need rerun | `62_store-suite.log` |
| `cargo test -p boop-harness --lib` | 169 passed, 1 ignored | `100_harness-boundary-suite.log` |
| `cargo test -p boop-acp --lib` | 50 passed, 6 ignored | `101_acp-boundary-suite.log` |
| `cargo test -p boop-proc --lib` | 159 passed | `95_proc-boundary-suite.log` |
| `cargo test -p boop --bin boop` | 103 passed | `102_cli-boundary-unit.log` |
| `cargo test -p boop --test main` | 120 passed | `103_cli-boundary-integration.log` |
| `cargo test -p boop --test main -- t1_harness_boundaries::` | 2 passed after test-file classification correction | `106_harness-guard.log` |

Seven adapter tests remain explicitly ignored for authenticated or machine-dependent
environments. Full final affected-package and feature gates remain pending.
Before/after logs are retained: incident registration 5 passed / 3 failed before
fix; PID projection failed in `75_projector-pid-before.log` and passed in
`76_projector-pid-after.log`. Fixture failures and telemetry write-lock failure
were fixed; CLI integration passed 118/118 in `85_cli-integration-isolated-after.log`.
Fixtures retain HOME/CODEX_HOME and use Boop reader/config/database overrides.

## Authenticated Codex state

Report 2 has actual transcript receipts. Wrapped route `codex-1828`, parent
`probe-parent`, received idle and busy nonces, changed Luna/low to Terra/medium/high,
compacted, exited and resumed in another process. Initial thread:
`01a081bf-5d5a-7f23-a33d-81d349d9d56f`. `/clear` created
`01a081d7-85ca-7ca1-a8ce-f3ce84dadc50` and retained route, parent and Boop trace.
The cleared thread received a nonce and resumed in another process. Clear reset
Codex to Astra/xhigh; a supported per-thread change selected Luna/low before a
bounded prompt. No user config was changed.

Concurrent same-cwd test `codex-process-51263` used thread
`01a081e4-1c58-7f83-9e4a-24d2d414bdf9`, independent trace and real PTY with TMUX
variables removed. Each received exactly one distinct nonce and answer, with zero
cross-transcript occurrences (`67_concurrency-verdict.json`). Both fixtures stopped
at 16:48:15 UTC and private sockets disappeared. Dead test tmux cells remain.

Test-only message `m-322f985c` awaits resume of the cleared thread in test pane
`%1828`, session `boop-proof-owned5-01a08191`. Expected answer:
`ACK_BP_STALE_RESUME_01a08191_1`. This can prove stale-route recovery and the PID fix.
Old proof processes used binary SHA-256
`3ef0dc3101e49ce0526948d149ce7ffc1dacfc975591371a0c24f599f537e99f`.
Record a new launch manifest before launching the current executable.

Retained live failures: initial guardian misbinding, missing resume broadcast,
synchronous proxy timeout and a pre-fix killed-wrapper orphan. The verified
owned orphan group 5943 was cleaned up. SIGKILL/restart and remaining supervisor/
child cases are open. Only Codex has live authenticated coverage. All five
installed CLI help probes succeeded (`88_available-harnesses.json`).

## Continuation checkpoint, 2026-09-08 18:35 UTC

Current committed head is `bb5445b`. Raw receipts `124` through `127` prove
automatic delivery of held `m-322f985c` on explicit resume, retained live PID/pane,
and actual Terra/high settings with answer to `m-46c1c14b`. The narrow authenticated
retry test passed in `134_authenticated-retry.log`: one native user message and
one answer before and after two `AlreadyAccepted` attempts. Earlier compile and
raw-reader failures remain recorded in `128`, `129`, and `131`; the corrected test
uses production `Harness::messages`. Launch `133_retry-resume-launch.json` records
the executable for that run. This is partial lifecycle coverage.

Killing only verified test-owned backend group 12024 reproduced wrapper exit 1
with `native TUI observation failed: read backend frame` (`136`). Uncommitted
`cli/control.rs` now processes observed identity/settings before deciding whether
to restart, monitors the owned backend, and reuses the existing bounded relaunch
path for backend/observer failure. Control tests pass 11/11 (`137`); build passes
(`138`). Live after-fix launch is `139_restart-launch.json`, test pane `%1828`,
same cleared thread, bounded to 480 seconds. After-fix crash verification remains
pending at this checkpoint. Test source `tests/3_live_codex.rs` is uncommitted.

Acceptance now explicitly requires one central executable gate with shared
lifecycle scenarios/assertions behind a harness trait and four configuration
rows: Claude, Codex, OpenCode, ccz. ccz must share Claude operations. Deterministic
checks must enter CI, with an explicit authenticated wrapper mode and per-scenario
PASS/FAIL/UNSUPPORTED/BLOCKED results. The existing `scripts/door-e2e.sh` uses a
production newest-thread lookup and obsolete CLI calls; do not run it as a fixture.
Migrate its entry point into the isolated central gate. No live assurance for
Claude/OpenCode/ccz is established yet. No native collaboration tools are allowed.

Parent milestone `m-9e8d80a4` also remained held for lane supervisor (`130`).

## Central gate continuation, 2026-09-08 19:03 UTC

Committed `8da1197` contains Codex backend restart and authenticated retry proof.
Subsequent work is uncommitted. Preserve all diffs, including new
`scripts/0_regression_gate.sh` and `tests/4_lifecycle_gate.rs`.

`just boop-check deterministic` runs Boop package contracts/integration, dl6 and
no-default-feature checks; CI now calls it and excludes those packages from the
remaining workspace step. It has NOT yet been executed as a complete gate.
`BOOP_E2E_ROOT=<task-owned-root> just boop-check live` runs Claude, Codex, OpenCode,
ccz through one `LifecycleHarness` scenario driver. ccz shares `Claude` operations.
The old `scripts/door-e2e.sh` delegates to this command and no longer selects a
production newest transcript. The live runner deliberately fails while remaining
scenarios are unimplemented. Do not present it as complete assurance.

New executable regressions and live findings:

- `Claude::session_by_id` passed a projects directory into a home-root helper.
  Isolated subprocess reader regression failed (`152`) and passes (`153`). The
  adapter also now reports actual assistant model metadata from its native file.
- OpenCode ignored the route's HTTP endpoint. Route-specific contract failed
  (`150`), and six HTTP door tests pass (`154`). Owned launch now allocates a local
  endpoint and retains its backend in `NativeTuiPlan`; explicitly addressed
  existing servers remain borrowed. Explicit session arguments select resume.
  Additional ownership/resume regression coverage remains required.
- The first four-entry run is `155_central-live.log`. Claude and ccz answered
  native peer nonces, but had no Boop trace. Codex answered but the test used the
  wrong exit control; it now uses the previously verified Ctrl+D. OpenCode
  received the nonce but its provider rejected GLM-5.3-Highspeed subscription
  access. The user's actual configured model is zai-coding-plan/glm-4.6; the
  adapter had silently substituted the provider default. That substitution is
  removed, with a deterministic preservation test added, not yet rerun.
- `bind_native_session` now owns trace/PID/session binding for legacy registry
  observations and Codex events, replacing `record_pane`. Claude fresh/nonce/
  retry/exit/resume passed in `157`, root `shared-live/claude-22765`.
- Claude compact completed in `158`, but the next test nonce hit default door
  throttling. Fixture-only BOOP_DOOR_FLOOR=30 now matches earlier bounded trials.
- `159` passed compact plus resume, then failed clear rebind within 25 seconds.
  The registry poll now identifies a new session by exact frontend PID and uses
  the canonical binding path. After-fix run `160_shared-claude-clear-after.log`
  is active at this checkpoint (unified exec session 80378).

Raw matrix roots are under the existing raw root's `shared-live/`. Each new
launch records script/command/build/hash with a numeric sequence. All fixture
tmux servers have unique `boop-e2e-ENTRY-PID` sockets. Their Drop closes only that
server. Verify backend/frontend cleanup explicitly before claiming the cleanup
scenario passes. The previous `%1828` manual test has expired its bounded launch.
No audit lane or protected parent has been closed.

Remaining shared scenarios include busy receipt, model/effort changes, process
restart/crash, concurrent isolation, parent completion and explicit cleanup
assertions. Compact/clear/resume are currently implemented for native JSONL
adapters but still being verified. OpenCode compact observation and provider
execution remain open. Native child operations must stay within the harness
boundary. Existing narrow `3_live_codex.rs` retry test remains to consolidate into
the shared driver. Reports 0/1/2/4/5 and hashes still need final refresh.

Latest parent milestone `m-1fcb2672` was held for lane supervisor (`146`).

## Gate checkpoint, 2026-09-08 19:15 UTC

The full central deterministic command passed (`169`), with the final disabled
feature import cleanup also passing (`172`). Default Boop integration: 124 passed,
2 authenticated tests ignored. dl6 Boop integration: 127 passed, 3 ignored.
Boop unit: 105 passed; harness: 173 passed/1 ignored; ACP: 50 passed/6 ignored;
proc: 159 passed; store: 176 passed. Mux/turnvis and other targets also passed in
the central log. The dl6 missing capability field and no-default compilation
failures (`163`, `166`, `167`) were fixed. DeliveryRow/Store::delivery_rows moved
to core ident storage with a compatibility query re-export; host/concatmap are
gated on their reader dependency. `dl6` explicitly enables `agent-read`.

Claude full implemented shared sequence passed through clear and resume in
`160`, raw `shared-live/claude-54743/matrix.json`. Its final result is FAIL because
required scenarios are still unimplemented. New explicit process cleanup and
native prerequisite checks have been added since that run and need live rerun.
The test publisher is now registered as a separate coordinator route before
launch. Preserve these distinctions when reporting prior receipts.

OpenCode attach help (`170`) has no --model flag. Native model enumeration (`171`)
lists glm-4.7, glm-5-turbo, glm-5.2, glm-5.2-highspeed, glm-5.3, glm-5.3-flash and
glm-5.3-highspeed under zai-coding-plan; configured glm-4.6 is absent. The removed
default substitution caused the Highspeed subscription rejection. Next work must
honor explicit launch model selection through the existing owned backend and
observe actual execution, without user config changes or new paid authority.

The audit lane remains joinable. Parent milestone `m-a6a15aa1` is still held for
lane supervisor (`165`). Full feature inventory and current AST/hashes refresh,
shared busy/settings/crash/isolation/parent cases, and complete per-entry live
results remain required. No final completion is claimed.

## Remaining work

1. Commit the passing delivery cluster, then resume the test TUI for live proof.
2. Review legacy non-Codex discovery/claim paths and remaining CLI contradictions.
3. Complete bounded live stale recovery, abnormal exit, supervisor restart/reattach,
   child completion, actual parent notification and duplicate/retry proof.
4. Complete command/alias/hidden/config/identity/telemetry/feature inventory.
5. Commit portable opt-in authenticated E2E coverage, finish affected-package
   gates, update hashes and PASS/FAIL/BLOCKED ledger, clean task fixtures only.

Raw drivers `0_launch_baseline.bash` and `4_resume_live.bash` evaluate generated
Bash shell-init. `1_capture_live.py` records selected thread/turn data, route, trace,
attributes, process tree and delivery ledger. Node helpers use the cached `ws`
package. Complete user transcripts are not committed.

## Parent push failures

Milestones `m-6e05b74e`, `m-4531df56`, `m-1d4a403e`, `m-24fbea68` and latest
`m-a3b88f59` (17:24:35 UTC, `86_parent-milestone.txt`) were held for
`lane supervisor` on the protected parent. No parent transcript receipt is claimed.
No repair or fixture mutation has been applied to that route or pane.
