---
created: 2026-09-14
updated: 2026-09-14
type: feature
status: open
priority: normal
---

# Stamp invocation events with exact build identity

## Description

Preserved uncommitted candidate as archival commit 9e7d0fc6cf56b06178cdc61a49d978bcfebcd4c6, ref archive/boop-cleanup-20260914/wip/fix/f41-boop-invocation-build-id. BuildInfo/BUILD_INFO adds compile-time version, exact SHA and timestamp to paired invocation event JSON. Main has invocation logging and argv redaction, but this structured build stamp is absent. Review build-script reproducibility and port only the stamp plus meaningful real SQLite tests. Snapshot is unvalidated; old worktree removed.
