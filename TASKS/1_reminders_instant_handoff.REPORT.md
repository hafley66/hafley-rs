# Reminder and Instant final handoff, 2026-09-06

| Repository | Checkpoint | Delta | Publication |
|---|---|---|---|
| Boop | `f0a9566` recurring reminders | 13 files, +1386 / -13 | Pushed to origin feature/boop-reminders-instant |
| Boop | `596c9f4` favorite CLI persistence | 2 files, +137 / -0 | Pushed to the same lane branch |
| Instant | `5b8ee40` existing turn widget / favorite reasons | 18 files, +465 / -144; two PNG receipts | Local isolated branch |
| Instant | `f7a647f` runtime provenance / whitespace | 2 files, +15 / -1 | Local isolated branch |

Boop implementation and exact proposed Game3 registration/add/run/cancel commands:
[0_recurring_reminders.REPORT.md](0_recurring_reminders.REPORT.md).
Instant interface, gates and native/browser receipts:
`/Users/chrishafley/projects/instant-worktrees/boop-reminders-instant-integration/instant/docs/0_boop_turn_widget.REPORT.md`.

Boop: 400 library/binary regression tests, 3 reminder CLI integration tests, and
1 favorite CLI persistence test passed. The two new reminder runtime modules
contain 458 lines before their test modules. Existing route, mailbox, supervisor,
door budget and completion interfaces carry delivery.

Instant: strict TypeScript, production build and cargo-check passed. Targeted
checks passed 5 Vitest tests, 1 Rust persistence test, 1 browser favorite widget
spec and 1 actual native WKWebView favorite IPC/rendering spec. Two additional
real-transcript browser attribution tests passed; the third has a 26-pixel
Codex snapshot mismatch reproduced unchanged on base `6f2786c`. No snapshot was
updated to conceal that failure. The native build used the isolated Boop path
dependencies, one compiler job, no global install, and an isolated test app/store.

| Harness | Actual live receipt | Fixture / skip boundary |
|---|---|---|
| Codex | `m-f37e14c4`: appended, accepted-by-harness, turn-ended; exact token echo | Earlier offline `m-5ee3d93d` was consumed only after resume; admission alone did not prove read |
| Claude | `m-c6f7d04d`: appended, accepted-by-harness, turn-ended; exact token echo | Second occurrence was cooled off by the existing repeated-body budget |
| OpenCode | None | Adapter fixture passed; live paid-provider invocation skipped |
| Kimi | None | Mailbox/supervisor fixture passed; live ACP permission auto-grants blocked the probe |

All disposable reminder schedules expired and recipients completed/stopped or
detached. The existing user's Codex daemon was preserved. ACPX coordinator mode
is held unavailable because its existing queue enables approve-all permissions.
No pending or accepted message is reported as recipient read without transcript
evidence. Cancellation/expiry stop new submissions; accepted offline harness
work can be consumed later.

The verified replacement sequence was sent to the coordinator as `m-d456eeed`,
accepted by its Codex door. No Game3 reminder was added and cron was not changed.
The existing `game3-overnight` route was assigned its explicit actual thread via
supported registration. `codex-tsi` was preserved. No Game3 code was edited.

Primary Boop overlap remains `crates/boop/src/main.rs`; other preexisting dirty
Boop files and both primary checkouts were preserved. No merge or integration
was performed. Instant was not pushed to a separate remote. The temporary
baseline verification worktree was removed after recording the reproducible
snapshot failure.

## Boop file deltas through 596c9f4

| File | Added | Removed |
|---|---:|---:|
| `TASKS/0_recurring_reminders.REPORT.md` | 190 | 0 |
| `crates/boop-proc/src/deliver.rs` | 92 | 1 |
| `crates/boop-proc/src/supervise.rs` | 135 | 6 |
| `crates/boop-store/src/1_reminder.rs` | 454 | 0 |
| `crates/boop-store/src/bus.rs` | 20 | 4 |
| `crates/boop-store/src/ident.rs` | 5 | 1 |
| `crates/boop-store/src/lib.rs` | 2 | 0 |
| `crates/boop/src/cli/1_reminder.rs` | 269 | 0 |
| `crates/boop/src/cli/job.rs` | 4 | 1 |
| `crates/boop/src/cli/mod.rs` | 21 | 0 |
| `crates/boop/src/main.rs` | 8 | 0 |
| `crates/boop/tests/1_favorite_reason.rs` | 135 | 0 |
| `crates/boop/tests/1_reminder.rs` | 184 | 0 |
| `crates/boop/tests/main.rs` | 4 | 0 |

## Instant file deltas through f7a647f

| File | Added | Removed |
|---|---:|---:|
| `artifacts/1_favorite_reason.png` | - | - |
| `artifacts/native-e2e/1_favorite_reason.png` | - | - |
| `docs/0_boop_turn_widget.REPORT.md` | 121 | 0 |
| `e2e-native/1_favorite_reason.spec.ts` | 44 | 0 |
| `e2e/1_favorite_reason.spec.ts` | 51 | 0 |
| `ipc/commands.json` | 1 | 1 |
| `src-tauri/src/0_boop.rs` | 2 | 78 |
| `src-tauri/src/1_boopFavorites.rs` | 145 | 0 |
| `src-tauri/src/lib.rs` | 6 | 3 |
| `src/00a_terminalIntersection.ts` | 1 | 1 |
| `src/0_askText.test.ts` | 24 | 0 |
| `src/0_askText.ts` | 41 | 0 |
| `src/0_boopCandidateWindow.ts` | 17 | 0 |
| `src/chrome.ts` | 8 | 42 |
| `src/favorites.ts` | 14 | 18 |
| `src/generated/native.ts` | 2 | 0 |
| `src/tablepanels.tsx` | 1 | 0 |
| `wdio.native.conf.ts` | 1 | 1 |
