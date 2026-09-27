---
created: 2026-09-14
updated: 2026-09-14
type: bug
status: fixed
priority: high
closed: 2026-09-14
closed_by: codex
---

# Resolve Claude parked-job conversations for pane turn attribution

## Description

An interactive Claude registry record can keep an old sessionId while parkedJobId names the job currently shown in its tmux pane. The background registry record has matching jobId and the current sessionId, without tmux metadata. Boop currently drops both job fields and attributes the pane to the old transcript even when current turns are already ingested. Resolve through exact explicit job identity with live-process and ambiguity checks; preserve stored session identity and normal pane behavior. Add public pane-resolution regression coverage using synthetic registry files and real process liveness. Worker fix-claude-parked-job; parent owns install and Instant rebuild.

## Resolution

### 2026-09-14T13:18:13Z · @codex

Implemented Claude-specific presented-session resolution in commits 6ca262ce,fa9a48c7,4e903aea. Only an interactive host with an explicit parkedJobId redirects, and only to one live bg record with exactly matching jobId. Missing/dead/ambiguous matches retain the host; native registry facts and stored route identities remain unchanged. Public pane and route lookup covered, with child-process isolation for the real-registry test. Parent tests: 212 library passed (2 ignored), bench_grid 2 and native_model_receipt 1 passed; cargo check boop and release build passed; Instant and instant-serve rebuilt. Live isolated Instant RPC probe for compiler pane returned the old host session before fix and the exact current parked job after fix, including a job change during verification. Signed Boop installed at product 4e903aea; existing unrelated checkout dirt is reflected in the binary version suffix. Native app reload triggered via existing Tauri watcher.
