---
created: 2026-09-18
updated: 2026-09-18
type: feature
status: open
priority: normal
epic: extract-parity-move-rename
labels: [extract]
---

# extract diff: fact delta between two commits, in-crate

## Description

## Description

User 2026-09-18: "i must make extract on its own very capable". Steps 0-3 fit the current AGENTS.md law; step 4 collides with `crates/sprefa-extract/AGENTS.md:32` (no delta resolver in this crate) and needs a user amendment first.

```
step 0  snapshot(Revision::Commit(A)) via soopy SourceTree   4_watch.rs:140-146 hardcodes Revision::Worktree; soopy _0_types.rs:239 has Commit(ObjectId)
step 1  snapshot(Revision::Commit(B))
step 2  diff_snapshots(a, b) -> [Added, Removed, Changed]    4_watch.rs:293
step 3  phase 1 on the delta blobs only                      bytes via cat_blob 0_query.rs:51; ReceiptStore::replace retract/assert 4_watch.rs:350
step 4  resolve over the delta -> edges gained / lost / origin drift
```

Diff key for step 4: `(caller_path, caller_name, callee_path, callee_name, kind, resolution_origin)`, never a byte span. Same shape for `resolved_type_edge`, `resolved_import`, `file_unresolved`.

Cheap step 4: full resolve at B, in-memory set difference against A's rows, one process, no sqlite. Semi-naive step 4 is what dl8 `_6_eval` does; porting it here is the amendment.

## Acceptance Criteria
- [ ] `extract diff --from <sha> --to <sha>` prints step-2 blob deltas and step-3 receipts without a checkout
- [ ] AGENTS.md amended or step 4 explicitly deferred to dl8 (user call)
- [ ] step 4 output on the 18-file CTF corpus between two real commits, keyed as above

## Decisions

### 2026-09-18T13:27:03Z · @chris

Long run (user 2026-09-18): sprefa (dl8) will generate large portions of the extract foundation (types, sqlite, soopy usage, joining code); extract becomes the first user of the dl6 namespace primitives in dl8. Until then extract grows the capability in-crate; AGENTS.md:32 amendment still pending.
