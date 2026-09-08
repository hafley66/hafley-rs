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
- Current wrapper cluster: noninteractive passthrough, stable named ownership,
  observed settings on automatic resume and atomic process detachment. Wrapper
  regression passed (`108_wrapper-passthrough-after.log`), harness 171 passed / 1
  ignored (`109_harness-wrapper-suite.log`), control 11 passed (`110_native-control-suite.log`).
  New delivery retry reproduction calls the transport three times for one message
  (`111_delivery-retry-before.log`); its fix is next.

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

## Remaining work

1. Commit the passing wrapper cluster, then fix the delivery retry reproduction.
2. Reproduce and fix concurrent/repeated delivery of one message. ACPX admission
   and hook/supervisor fan-out remain separate. Analyze child completion marking
   when parent delivery holds. Review legacy non-Codex discovery/claim paths.
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
