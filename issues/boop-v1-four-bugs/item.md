---
created: 2026-10-01
updated: 2026-10-01
type: bug
status: fixed
priority: normal
epic: burndown-2026-10
labels: [boop]
closed: 2026-10-01
---

# boop v1: fix wait-on-failed-lane, shout tally, split store, orphan supervisor

## Description

Contract test first per bug (boop2 tests/), then Rust fix in crates/boop*. In progress (writer). Held-mail timeout is by design.

## Comments

### 2026-10-01T21:53:55Z · @claude-375

Merged ab59e5d4 (hafley-rs) + boop2 main cd9defd. Suite 22: 18 ok, 4 skip, 0 fail. Bug 4 not reproduced as stated; fixed double result row on server death (end latch). Open: kimi unreachable by shout (no live-session registry).
