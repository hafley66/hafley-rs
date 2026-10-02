---
created: 2026-10-01
updated: 2026-10-01
type: bug
status: open
priority: normal
epic: burndown-2026-10
labels: [boop]
---

# transcript sync aborts: trace join observation budget exceeded 96821 > 10000

## Description

After the live store reached schema 40, every sync logs: Error: trace join observation budget exceeded: 96821 > 10000 (crates/boop-store/src/0_trace_identity.rs:109, from 7c9b54bf). Commands still run (tag recent OK), but projection aborts. Check whether schema 40 changed what the join reads.
