---
created: 2026-09-15
updated: 2026-09-15
type: feature
status: open
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

## Decisions to resolve
Daemon automatic startup versus explicit enablement; new ordinary session versus managed lane/worktree; frontmatter format and filename grammar. Proposed precedence: CLI, frontmatter, filename, defaults. A daemon is a requested design direction, not implemented infrastructure.

## Acceptance
- [ ] Inline and file submissions share IDs, metadata resolution and receipts.
- [ ] Existing and explicit new recipients work through supported real harnesses.
- [ ] Interrupted imports and restarts recover without accidental duplicate sends.
- [ ] Stored, queued, accepted and uncertain delivery remain distinguishable.
- [ ] Large Markdown body and attribution survive delivery unchanged.
