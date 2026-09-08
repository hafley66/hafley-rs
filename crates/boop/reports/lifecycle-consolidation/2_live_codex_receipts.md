# Live Codex receipts

Work in progress. Actual transcript receipts are listed separately from queue admission.

## Current executable matrix, run 256

The central four-harness command completed with a nonzero aggregate result.
Source implementation: `6230786`; exact frozen executable hashes and commands
are in each entry’s numbered launch manifests. Native receipt assertions include
route kind, parent, pane/PID, thread, trace, model/effort, intended TUI display,
accepted delivery and two AlreadyAccepted retries with unchanged counts.

| Shared scenario | Claude | Codex | OpenCode | ccz |
| --- | --- | --- | --- | --- |
| `native_executable` | PASS | PASS | PASS | PASS |
| `fresh_wrapper_identity` | PASS | PASS | PASS | PASS |
| `idle_receipt_and_accepted_retry` | PASS | PASS | PASS | PASS |
| `busy_receipt` | PASS | PASS | PASS | PASS |
| `exit_and_resume_across_processes` | PASS | PASS | PASS | PASS |
| `stale_route` | PASS | PASS | PASS | PASS |
| `compact` | PASS | PASS | PASS | PASS |
| `resume_after_compact` | PASS | PASS | PASS | PASS |
| `model_and_effort_change` | BLOCKED | PASS | BLOCKED | BLOCKED |
| `settings_across_resume` | BLOCKED | PASS | BLOCKED | BLOCKED |
| `abnormal_exit` | PASS | PASS | PASS | PASS |
| `backend_restart` | UNSUPPORTED | PASS | PASS | UNSUPPORTED |
| `clear_new_session` | PASS | PASS | FAIL | PASS |
| `resume_after_clear` | PASS | PASS | BLOCKED | PASS |
| `concurrent_session_isolation` | PASS | PASS | PASS | PASS |
| `child_completion_parent_receipt` | PASS | PASS | PASS | PASS |
| `concurrent_process_cleanup` | PASS | PASS | PASS | PASS |
| `process_cleanup` | PASS | PASS | PASS | PASS |

Matrices: `claude-36005/matrix.json`, `codex-52478/matrix.json`,
`opencode-72907/matrix.json`, `ccz-92874/matrix.json`. Condensed machine results:
`263_four-harness-results.json`. Codex passed in 183.02 seconds; Claude, OpenCode
and ccz completed in 173.62, 244.02 and 190.81 seconds respectively.

Claude/ccz settings are BLOCKED because installed controls persist user defaults;
restoration of the two keys changed by trial 201 is still pending. Their direct
native launch plan has no separately owned backend, hence UNSUPPORTED for that
specific backend-restart operation. Frontend crash/resume is verified.

OpenCode clear is FAIL: the TUI returned home but its route did not rebind within
25 seconds. The driver exited that test frontend, explicitly resumed the prior
conversation, and continued isolation/completion checks. Its settings controls
are unverified. No failed or blocked row contributes to a passing entry result.

The ccz isolation failure in trial 241 remains recorded in handoff. Current
Claude/ccz launch arguments restrict built-in tools to Bash and exclude MCP.
Native tool audits `263_claude-tool-isolation.json` and
`263_ccz-tool-isolation.json` found only Bash in the recorded current threads.

OpenCode installed `/doc` was captured from the test-owned backend in
`shared-live/opencode-85957/native-api.json`. It includes session model controls.
Native slash commands are documented in [OpenCode TUI documentation](https://opencode.ai/docs/tui/);
documentation alone does not establish live execution or binding correctness.

## Incident reproduction, before changes

Installed Boop SHA-256:
`84bea1f1ee1ecee51e9f1c28f209859de6009556ed224c00d69fdae5058db085`.
Codex installed version: `0.153.4`.

Test-owned route `incident-parent`, isolated `BOOP_DB`/`BOOP_MAIL_DIR`, absent
test Codex state DB/socket, test tmux session `boop-lifecycle-repro-01a08191`,
pane `%1812`. The test tmux session was removed after capture.

| Step | Actual result |
| --- | --- |
| Agent register coordinator with harness/cwd | Exit 0; no session ID |
| Incoming mail `m-5a8d1783` | Held: no live Codex session |
| Lane patch with `%1812` | Exit 0 while printing refusal |
| Lane patch with session name + explicit test thread | Exit 0; route kind changed to lane; omitted metadata erased |
| Incoming mail `m-2004731e` | Held: lane supervisor, although this route has none |

Raw command/output/database receipts:
`/private/tmp/boop-lifecycle-consolidation-proof-01a08191/0_incident-baseline.json`.
These reproduce routing failures. They do not establish transcript receipt.

Deterministic regression rerun: **PASS**, 8 `registry_kinds` tests, raw
`4_regression-after.log`. Authenticated transcript receipt remains separate.

## Authenticated acceptance cases

| Case | Actual result and raw receipt |
| --- | --- |
| Fresh generated Bash wrapper | **PASS** in pane `%1828`, route `codex-1828`, parent `probe-parent`, coordinator, persisted thread `01a081bf-5d5a-7f23-a33d-81d349d9d56f`; `29_launch.json`, `30_idle_receipt.json` |
| Idle nonce | **PASS**: `m-54ff5622`, exactly one user turn and `ACK_BP_IDLE_01a08191_3`; `30_idle_threads.json` |
| Busy nonce | **PASS**: `m-3ab7a194` sent while status was `active`, answered after bounded `sleep 8`; `31_busy_before_send_receipt.json`, `31_busy_after_threads.json` |
| Model/effort change | **PASS**: supported `thread/settings/update` changed Luna/low to Terra/medium/high, then Luna/low after resume. Actual turn contexts, replies and route model confirm changes. Earlier stale-route failure retained; `35_model_receipt.json`, `38_effort_receipt.json`, `49_settings_receipt.json` |
| Explicit compact | **PASS**: actual `/compact`, UI `Context compacted`, persisted `compacted` event; same thread/session ID, subsequent `m-e01dfb45` nonce answered; `36_compact_receipt.json`, `37_postcompact_threads.json` |
| Clean exit | **PASS**: Ctrl-D, exit 0, owned backend socket removed; `40_clean-exit.txt` |
| Explicit process resume after compact | **PASS**: same history/thread, Terra/high, route rebound; previously held `m-27e70b21` arrived and was answered automatically; `48_resumed_receipt.json` |
| Clear/new | **PASS for `/clear`**: new thread `01a081d7-85ca-7ca1-a8ce-f3ce84dadc50`, same Boop trace and parent; new-session default Astra/xhigh was observed, then explicitly set to Luna/low before nonce `m-18364e1a`; `51_cleared_receipt.json`, `53_postclear_receipt.json`, `53_trace-identity.json` |
| Resume after clear | **PASS**: new process resumed the cleared thread and answered `m-1215d682`; `64_clear-resume-launch.json`, `67_concurrent-A_receipt.json` |
| Concurrent same-cwd sessions | **PASS**: distinct routes, sockets, threads and traces; one request/answer per nonce, zero crossover. Second wrapper had no tmux identity variables; `65_concurrent-routes.json`, `67_concurrency-verdict.json` |
| TERM and normal cleanup | **PASS for backend cleanup**: test wrapper PID 51263 verified before TERM; both test TUIs exited, both owned sockets removed; `69_test-exits.json` |
| Stale route | **PARTIAL**: `m-322f985c` held with exact missing-socket error after exit; no wrong transcript targeted. Route transport/status cleanup remains open; `71_stale-send.txt` |
| SIGKILL/restart/reattach, child completion/parent receipt, concurrent retry admission | **PENDING** |

Initial live failures are retained: scratch cwd trust prompt, rejected socket
directory layouts, ephemeral guardian misbinding (`23_idle-send.txt`), backend
orphan after a pre-fix wrapper timeout (`27_timeout-cleanup.json`), and a blocked
synchronous websocket forwarder (`45_resumed_receipt.json`). The replacement
uses Tokio/Tungstenite with independent directions. Eleven Codex adapter tests
pass, including simultaneous 512 KB frames (`47_codex-adapter-tests.log`).

Only task-owned Codex conversations were read for these receipts. No receiver
used `boop wait` or polled a mailbox to obtain the nonce. The driver inspected
the owned backend and its actual rollout afterward. Full raw receipts remain
outside Git under `/private/tmp/boop-lifecycle-consolidation-proof-01a08191`.
# Resume, retry and owned backend recovery, 2026-09-08 18:37 UTC

Raw storage remains `/private/tmp/boop-lifecycle-consolidation-proof-01a08191`.
These receipts extend the earlier matrix; other harness live coverage remains open.

| Case | Actual result | Evidence |
|---|---|---|
| Held stale-route nonce on explicit resume | PASS: `m-322f985c` answered automatically in cleared thread `01a081d7-85ca-7ca1-a8ce-f3ce84dadc50` | `124_stale-resume-launch.json`, `125` capture |
| Observed settings after resume | PASS: `m-46c1c14b` answered with actual Terra/high turn metadata and retained PID/pane | `126` settings RPC, `127` capture |
| Retry after recorded acceptance | PASS within tested window: production `Harness::messages` observed one user message and one answer before/after two `AlreadyAccepted` delivery attempts, five-second observation | `133_retry-resume-launch.json`, `134_authenticated-retry.log`; test `tests/3_live_codex.rs` |
| Backend SIGKILL before fix | FAIL: wrapper exited 1 with `native TUI observation failed: read backend frame` | `135` capture, `136_backend-crash-before.json` |
| Backend SIGKILL after fix | PASS: same wrapper relaunched native frontend/backend, retained thread/route/parent/trace and Terra/high; PID 22115 became 24047, old socket removed | `139_restart-launch.json`, `140` before, `141_backend-crash-after.json`, `142` after |
| Nonce after automatic backend restart | PASS: `m-17d20441` received and answered in real turn `01a0824e-9bae-7df2-935c-42e1fd9231a2` with `ACK_BP_BACKEND_RESTART_01a08191_1` | `143_restart-nonce.txt`, `144_restart-answer_receipt.json` and native thread read |

All launches used the actual generated Bash wrapper in test pane `%1828`, parent
`probe-parent`, coordinator route `codex-1828`. Launch manifests record exact build
hashes. No receiver-side wait/poll/paste produced these nonces. Test assertions read
the native transcript after delivery. The remote-acceptance/local-ledger crash
window remains unverified; this does not establish arbitrary exactly-once transport.
# Shared four-entry runner checkpoint

The live command is `BOOP_E2E_ROOT=<task-owned-root> just boop-check live`.
It currently fails on unfinished scenarios. Raw roots below are relative to
`/private/tmp/boop-lifecycle-consolidation-proof-01a08191/shared-live/`.

| Entry | Executed shared cases | Current remaining result |
|---|---|---|
| Claude | `claude-54743`: fresh wrapper, inbound peer receipt, accepted retry suppression, graceful exit/process resume, compact/nonce, resume after compact, clear/new session/nonce, resume after clear all PASS | Busy/settings/crash/isolation/parent scenarios remain unfinished; latest cleanup assertion not yet rerun live |
| Codex | `codex-98246`: fresh wrapper and idle nonce/retry PASS | Runner used an unverified exit spelling and failed; corrected to previously verified Ctrl+D, shared sequence rerun pending. Earlier manual Codex proofs above remain separately labeled |
| ccz | `ccz-6954`: distinct ccz wrapper launch, actual peer nonce and retry suppression PASS | Shared run exposed missing trace binding before its fix; complete after-fix ccz sequence pending |
| OpenCode | `opencode-1495`: generated wrapper, owned HTTP backend and exact route binding PASS; native TUI displayed the inbound nonce | Provider BLOCKED GLM-5.3-Highspeed subscription access. This model came from an adapter fallback from configured `zai-coding-plan/glm-4.6`; fallback removed. Listed models (`171`) omit glm-4.6. Explicit supported-model launch work remains |

Claude `159` proves the pre-fix clear failure; `160` proves clear/rebind and
resumption after correction. The native registry's exact frontend PID identifies
the changed session; the Boop trace is retained. `152`/`153` record the exact
reader path regression. `161`/`162` record initial registry binding failure and
passing deterministic same-process transition/cleanup. None of these fixtures
uses a receiver-side `boop wait` to deliver a nonce.
