# Transcript sync v2

## Type surface

```rust
trait Harness {
    fn projection_version(&self) -> u32;
    fn sync_candidates(&self, known: &KnownSessions) -> Result<Vec<SessionRef>>;
    fn migrate_projection(
        &self,
        store: &Store,
        session: &SessionRef,
        from_version: u32,
    ) -> Result<SyncStat>;
    fn ingest(&self, store: &Store, session: &SessionRef, from: u64) -> Result<Ingested>;
}

struct SyncCursor {
    offset: u64,
    modified_ms: u64,
    projection_version: u32,
}

struct SyncDecision {
    new_session: bool,
    cursor_advanced: bool,
    source_modified: bool,
    projection_upgrade: bool,
}

struct SyncStat {
    written: u64,
    repaired: u64,
    dropped: u64,
    usage_written: u64,
    usage_updated: u64,
}
```

`SyncDecision` preserves every reason that places a session in the queue. A
source change and projection upgrade can therefore be measured on the same
session without collapsing either reason.

## Instance lifetimes

| Instance | Lifetime | Cardinality |
| --- | --- | ---: |
| `SyncFlight` | Entire pass | 1 per database |
| `KnownSessions` | Discovery and planning | 1 per pass |
| `AdapterPhase` | Discovery through reporting | 1 per registered harness |
| `SyncDecision` | Candidate planning through its projection log | 1 per candidate |
| `SyncCursor` | One session transaction | 1 per projected session |
| `SyncStat` | Session transaction, then folded into pass totals | 1 per projected session |

## Storage sequence

1. Acquire the database-specific single-flight lock.
2. Open and migrate the store.
3. Read all known cursor metadata with one `known_sessions` query.
4. Ask each adapter for candidates. The OpenCode adapter reads its compact
   `session` table, retains changed or projection-stale ids, then fetches
   message row cursors for those ids in bounded batches.
5. Construct one `SyncDecision` per candidate.
6. For each queued session, begin one SQLite transaction.
7. Read one `SyncCursor` for `(session_id, path_id)`.
8. Project discovery metadata once and insert the cursor row when it is new.
9. If the stored projection version is stale, run the adapter migration.
10. Ingest from `SyncCursor.offset`.
11. Write offset, source modification time, and projection version together.
12. Commit the session transaction.

The durable uniqueness boundary is the `sync_cursor` primary key
`(session_id, path_id)`. A projection version advances only in the transaction
that completed its migration and ingestion. `KnownSessions` retains the same
cardinality. Copied transcripts may share a harness session id across multiple
paths; path-qualified reads retain each cursor, while a session-only lookup
selects the path with the newest source modification time.

## Effect and query shape

```text
sync_all_budgeted
  Store::open
  Store::known_sessions                         1 query
  Harness::sync_candidates                      1 call per harness
    OpenCode session inventory                  1 query
    OpenCode message cursors                    ceil(changed ids / 500) queries
  for each queued session
    Store::cursor_state                         1 query
    Harness::migrate_projection                 only when version is stale
    Harness::ingest                             1 call
    Store::set_cursor_modified                  1 final cursor write
```

OpenCode projection repair uses bounded `IN` batches for exact message ids and
part ids. The steady-state candidate path issues zero message aggregation
queries when no OpenCode session changed.

## Telemetry

Each pass appends `start` and one terminal record to
`~/.agent/sync-trail.ndjson`.

Terminal `done` records include:

- phase durations: open, stale check, known inventory, route read, projection
- queue counts and independent decision reasons per harness
- written and repaired row counts
- database and WAL bytes before and after
- logical database growth, WAL growth, combined positive disk growth, and
  bytes per mutation
- budget yield status

An error return appends `aborted` with the last `SyncStage`, current queue
counts, and completed mutation counts. An unmatched `start` remains the signal
for process termination. Terminal matching uses `(pid, at_ms)` so PID reuse
does not hide a later killed pass.

Warnings are emitted at these thresholds:

| Signal | Threshold |
| --- | ---: |
| known-session inventory | 250 ms |
| adapter candidate discovery | 250 ms |
| one session projection | 250 ms |
| whole sync pass | 1,000 ms |
| positive database plus WAL growth | 16 MiB |

`boop debug` renders pass totals, adapter timings, decision counts, repairs,
database growth, aborted stage, killed passes, and single-flight deferrals.
