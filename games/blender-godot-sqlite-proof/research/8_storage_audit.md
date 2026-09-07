# Recycled-buffer and allocation audit

No tests were run. “Preallocated” and “recycled” below describe proposed
ownership; zero allocation remains unproved.

| Path | hidden copy/allocation or invalidation point | required measurement |
| --- | --- | --- |
| Rust state to ordinary SQLite rows | statement bind can copy text/blob with `SQLITE_TRANSIENT`; SQLite encodes records, updates B-trees, pages and journal/WAL | allocator count, `sqlite3_status64`, bound bytes, page/WAL growth per frame |
| Rust ring to virtual scalar columns | cursor/module/prepare/query structures may allocate; SQLite may sort/materialize joins; textual results may copy | warmed prepared statement versus reprepare, point/range/join allocation deltas and query plan |
| Virtual blob/text column | rusqlite `Context::set_result` lifetime behavior `[UK]`; SQLite `SQLITE_TRANSIENT` explicitly copies | exact wrapper source audit plus pointer/lifetime test under step/reset/finalize |
| Secondary lookup | SQLite cannot create indexes on a virtual table; custom index storage and `xBestIndex` plan are application-owned | point/range complexity and planner-selected plan for each predicate/order |
| Cursor versus ring recycle | an index into a slot becomes stale if writer wraps during scan | hold cursor across N publishes; generation guard must prevent overwrite or return deterministic busy/stale error |
| Transactional virtual table | undo log/savepoints and isolation can allocate and retain old slots; callback panics cross FFI | fault injection at begin/update/sync/commit/rollback and leak/poison audit |
| GGRS snapshots | `State: Clone` can deep-copy all vectors for each save/load; request vectors/history allocate `[UK]` | retained bytes and allocations at prediction windows 1, 8, 16, 32 |
| Rapier and ozz | Rapier pipelines/workspaces and ozz contexts reuse selected scratch, but callbacks/results/resource loads can allocate | after warmup, count each stage independently and force capacity high-water cases |
| Host upload | Godot conversions may allocate variants/arrays `[UK]`; wgpu `write_buffer` documents a native staging allocation | allocation count and copied bytes; compare reusable Godot packed arrays and wgpu `StagingBelt`/mapped ring |

SQLite explicitly documents that transient bind/result data is copied
([bind](https://www.sqlite.org/c3ref/bind_blob.html),
[result](https://www.sqlite.org/c3ref/result_blob.html)). Static/destructor modes
transfer a longer lifetime obligation and do not make arbitrary query results
safe to point into a slot that can be recycled. SQLite also documents that a
virtual table supplies its own indexing and cannot receive an additional
`CREATE INDEX` ([virtual tables](https://www.sqlite.org/vtab.html)).

## Proposed pass/fail audit

Warm every capacity with 1,000 ticks, reset counters, then run 100,000 ticks
without asset loads or statement prepares. Report allocations and bytes for
each named stage. A zero-allocation claim passes only when the counter is zero
on every tested tick and rollback depth, with capacity unchanged. A recycled
buffer claim passes only when pointer/capacity identities remain stable and
generation guards prevent overwrite. Repeat with maximal entities, contacts,
blend layers, SQL cursors, and prediction depth to expose growth. Fault-inject
OOM and SQL errors, then verify slot-reader counts and transactions unwind.

The test is **proposed, not executed**. Rusqlite result-copy behavior, SQLite
planner materialization, Godot bulk upload allocation, GGRS cloning costs, and
maximum Rapier/ozz scratch capacities remain `[UK]`.
