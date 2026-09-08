# Live Codex receipts

Work in progress. No live lifecycle case has passed yet.

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

Fresh wrapper, incoming nonce, busy/idle exactly once, process resume, model and
effort changes, compact, clear/new, resume after each, crash/rebind, concurrent
sessions, child completion and parent receipt: **PENDING**.
