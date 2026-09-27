---
created: 2026-09-15
updated: 2026-09-27
type: feature
status: fixed
priority: normal
---

# Boop Markdown message intake and background delivery

## Description

# Boop Markdown messages and background delivery

Status: proposed; requested 2026-09-15. Owner: hafley-rs, with Instant consumers.

## Requested behavior
Keep the agent-facing Boop CLI flow. Accept inline text or a Markdown file addressed to an existing or newly created agent session. Recipient/session/model/preset metadata can come from frontmatter or filename. A daemon discovers ready message files, assigns timestamped names, and delivers them, including to agents in user-owned terminal panes. Large coordinator messages retain their body and sender attribution.

## Proposed signatures
```text
submit_message(target, markdown, metadata) -> MessageId
import_ready_file(path) -> MessageId
deliver_message(id) -> DeliveryReceipt
```
Bodies: resolve metadata, persist message, resolve or explicitly create recipient, invoke existing delivery ladder, append receipt. CLI and daemon share this implementation.

## Lifetime, storage and uniqueness
One stable message ID spans import, retries, delivery and UI display. Use existing agent_mail and agent_delivery_transition. Import claims persist before file renaming; reconcile interrupted filesystem/database transitions on restart. Timestamp-plus-ID names prevent collisions. Atomic temporary-to-ready file publication avoids reading partial files. Content hashes detect changes; intentional repeat messages remain possible. Serialize delivery per recipient. Resume held work with bounded backoff. Query receipts before retrying uncertain submissions.

The proposed daemon owns intake and delivery scheduling; existing route/session creation and harness adapters remain the execution paths. Keep CLI one-shot delivery available. Unknown routes require explicit create metadata or return a held/error receipt. Existing sessions retain their model unless explicitly changed.

## Evidence
- hafley-rs/crates/boop-proc/src/deliver.rs: delivery ladder and receipt states; tmux fallback can paste a mailbox notice without submitting it.
- hafley-rs/crates/boop/src/main.rs: startup transcript sync and held-mail draining.
- hafley-rs/crates/boop-store/src/ident.rs: durable mail and delivery tables.
- ext/herdr/src/app/api/agents.rs: agent.prompt queues PTY text and Enter 300 ms later; optional wait observes agent state and does not establish message-specific completion.
- [Herdr API](https://github.com/herdrdev/herdr/blob/master/docs/next/website/src/content/docs/socket-api.mdx).
- [cmux events](https://github.com/manaflow-ai/cmux/blob/main/docs/events.md): reconnect cursors and replay.

## Decisions
2026-09-27: watcher is opt-in through `boop mail watch <dir>`. Unknown recipients require `harness`, `cwd`, and `worktree` frontmatter to create a managed lane; absent fields produce a refused mail row with the reason. Existing recipients keep their registered model and session. Frontmatter keys are `to`, `from`, `harness`, `cwd`, `worktree`, optional `branch`, and optional `preset`; `to` falls back to the Markdown filename stem.

## Acceptance
- [x] Inline and file submissions share message construction, metadata resolution and delivery receipts.
- [x] Existing routes use their registered session; explicit new recipients call lane creation through the selected harness adapter.
- [x] Message refs combine the relative filename and BLAKE3 content hash; restart recovery reuses the stored ID, with append-only receipts moved to `uncertain/`.
- [x] Appended, queued/held, accepted, refused and uncertain outcomes remain distinguishable in mail and transition rows.
- [x] Large Markdown body and sender attribution are retained unchanged.

## Reproduction receipt

2026-09-27: current source `boop mail --help` has only send, recv, and wait; `boop mail watch --help` returns `unrecognized subcommand 'watch'`. `crates/boop/src/cli/mail.rs` has no Markdown importer. Reproduced the intake defect before implementation.

## Implementation receipt

`boop mail watch <dir>` polls ready top-level `.md` files; `--once` runs one
scan. Frontmatter supports `to`, `from`, `harness`, `cwd`, `worktree`, optional
`branch` and `preset`; the filename stem supplies a missing recipient. Unknown
recipients without the three required creation fields produce a `refused` mail
row with the reason and move to `rejected/`. Explicit new recipients validate
the requested worktree and spawn through the existing lane path. Imports share
message construction, persistence, and the delivery ladder with inline sends.
The filename and content hash form the stable import key. Recovered imports
reuse the message ID; append-only receipts move to `uncertain/` without an
automatic second delivery.

Tests: frontmatter body preservation, a 10,000-line message and attribution,
unknown-recipient refusal, duplicate import ID preservation, append-only
uncertainty recovery, and CLI help example parsing. Workspace suite:
`cargo nextest run --workspace -j 2 --status-level fail -E 'not
(test(/e2e|live|tmux|tui_sigint|omp_live/))'`.
