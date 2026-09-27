---
created: 2026-09-14
updated: 2026-09-27
type: feature
status: deferred
priority: normal
---

# Bounded discussions across selected Boop sessions

## Description

Deferred follow-up from session messaging design. Reuse Boop send, mailbox storage, caller route identity, focus history and persisted recipient selection. A user in any agent TUI can initiate a bounded discussion with selected other live coordinator/native sessions, including a recent-focus shorthand. Define participant snapshot, who is prompted when, reply correlation, delivery/acknowledgment, turn or time budget, cancellation and completion semantics before implementation. Provide a terminal Markdown discussion viewer over the mailbox timeline; record user input as messages from the originating session. Decide whether timestamped Markdown files are an export/projection or authoritative storage based on existing code. Preserve one-shot multi-recipient send as a separate available interaction. Existing recipient selection is implemented; the bounded discussion protocol and viewer remain deferred. Review prior research in agent-messaging-research-20260913; no new broker or orchestration layer is selected by this issue.

## Comments

### 2026-09-27T04:38:50Z · @codex

Current contract receipt: the CLI has one-shot recipient selection and mail, while this card explicitly leaves bounded protocol and viewer deferred.
