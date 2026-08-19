---
created: 2026-08-19
updated: 2026-08-19
type: epic
owner: hafley66
status: open
priority: high
---

# boop-process: bash job control semantics + crate split (docs/design/boop-process.md)

## Description

## Description

Chris 2026-08-19: job control semantics from bash (`&`, `jobs`, `wait`, `kill`, `fg`, pipes, `trap`) as boop's one shape, and boop split into crates by responsibility. The analysis, the target verb table, the crate table, and the order of work are in `docs/design/boop-process.md`. Read that first; the cards below are its section 4.

## Cards

| # | card | size | blocked_by |
|---|---|---|---|
| 1 | boop-main-split (re-scoped: main.rs -> cli/*.rs by namespace, zero behavior change) | M | - |
| 2 | boop-crate-split (boop-store, boop-harness, boop-mail, boop-proc, boop-cli) | L | 1 |
| 3 | boop-job-namespace (`boop job`, `boop mail`, `boop me`; wait-all, kill vs rm, signal --children, attach, --timeout) | M | 2 |
| 4 | boop-mail-dir-global-flag + boop-hidden-verbs-retire | S | 3 |
| 5 | sprefa boop-hosted-in-dl6 (generated OpenAPI for /jobs /mail /me) | - | 3 |
