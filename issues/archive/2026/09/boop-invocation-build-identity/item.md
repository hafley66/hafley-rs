---
created: 2026-09-14
updated: 2026-09-26
type: feature
status: done
priority: normal
closed: 2026-09-26
commits:
- hash: 0fa04d04
  summary: 'feat(boop): stamp invocation build identity'
---

# Stamp invocation events with exact build identity

## Description

Preserved uncommitted candidate as archival commit 9e7d0fc6cf56b06178cdc61a49d978bcfebcd4c6, ref archive/boop-cleanup-20260914/wip/fix/f41-boop-invocation-build-id. BuildInfo/BUILD_INFO adds compile-time version, exact SHA and timestamp to paired invocation event JSON. Main has invocation logging and argv redaction, but this structured build stamp is absent. Review build-script reproducibility and port only the stamp plus meaningful real SQLite tests. Snapshot is unvalidated; old worktree removed.

## Resolution

### 2026-09-27T03:16:06Z · @issuectl

Receipt: 0fa04d04; `invoke::tests::start_and_finish_share_the_build_identity_in_sqlite` and `invocation_rows_group_by_distinct_build_sha` passed.
