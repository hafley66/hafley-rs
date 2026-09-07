# SQLite over bounded simulation buffers

Source review only. No experiment was run. `[D]` is official documentation,
`[S]` source/API observation, `[I]` inference, and `[UK]` unknown.

## Storage choices

| Boundary | Ownership and visibility | Indexing | copies, allocation, and constraints |
| --- | --- | --- | --- |
| Ordinary SQLite table, preferably `WITHOUT ROWID` for an application key | SQLite owns encoded pages. A transaction copies current Rust fields through binds; rollback history can be rows keyed by `(match_id, frame, entity_id)`. Separate connections see committed state; WAL readers retain the snapshot present when their read transaction began. [D] | Native primary/secondary indexes, statistics, joins, constraints. | Binding with `SQLITE_TRANSIENT` copies text/blob data before return; `SQLITE_STATIC` extends the caller buffer lifetime until rebind/finalize. B-tree/page/WAL work and row encoding remain. [D] |
| Read-only rusqlite virtual table over a Rust ring | Rust owns slots. A cursor borrows or pins one immutable generation while SQLite invokes `filter/next/column`. It exposes current or explicitly retained frames, with no SQLite durability. [D][I] | `xBestIndex` maps usable constraints and ordering to the ring's own lookup. SQLite cannot add `CREATE INDEX` indexes or triggers to a virtual table. [D] | Scalar columns can be returned without a simulation-row copy. Text/blob results copy when the wrapper uses transient result semantics; zero-copy requires a valid static/destructor lifetime across SQLite's consumption. Cursor/module objects and query execution can allocate. [D][UK] |
| Writable transactional virtual table | Rust owns storage and must implement update plus begin/sync/commit/rollback and optionally savepoints. [D] | Same built-in-index requirement. | Transaction isolation, undo storage, concurrent mutation, and cursor invalidation become application code. rusqlite 0.40.2 exposes `UpdateVTab` and `TransactionVTab`, but safety and allocation behavior require a pinned audit. [S][UK] |

SQLite describes a virtual table as callbacks over external storage and states
that `xBestIndex` may run more than once during prepare; `xFilter` receives the
selected plan, but need not follow every successful `xBestIndex`. Cursor state
therefore cannot borrow ephemeral planner data. Sources:
[virtual-table mechanism](https://www.sqlite.org/vtab.html),
[rusqlite 0.40.2 vtab module](https://docs.rs/rusqlite/0.40.2/rusqlite/vtab/),
[binding lifetime](https://www.sqlite.org/c3ref/bind_blob.html), and
[result lifetime](https://www.sqlite.org/c3ref/result_blob.html).

## Proposed ownership contract

```rust
struct FrameRing<const N: usize> {
    slots: [FrameSlot; N],
    published_generation: AtomicU64,
}

fn publish(ring: &mut FrameRing<N>, frame: FrameId, state: &SimState);
fn query_snapshot(ring: Arc<FrameRing<N>>, generation: u64) -> SqlSnapshotGuard;
fn persist(conn: &mut rusqlite::Connection, frame: FrameId, state: &SimState)
    -> rusqlite::Result<()>;
```

The simulation thread is the only writer. `publish` fills a recycled slot and
then publishes its generation. A virtual-table cursor holds an `Arc` plus a
generation guard, never `&mut SimState`. A slot cannot be recycled until its
reader count is zero. SQL updates are excluded from the hot simulation state in
the initial contract; ordinary-table persistence occurs after a confirmed-frame
boundary. This is proposed, not tested.

Rollback visibility is explicit: virtual SQL queries name a frame/generation;
they do not silently follow the mutable head. Ordinary-table readers on another
connection see complete commits, and WAL readers keep an unchanged snapshot
until their transaction ends ([SQLite isolation](https://www.sqlite.org/isolation.html)).

## Proposed experiment

Preallocate 16 frames of 512 fixed-size entity rows. Compare (a) prepared
ordinary-table upserts in one transaction and (b) an eponymous read-only vtab
with equality/range plans for `frame` and `entity_id`. Run 100,000 point reads,
range scans, joins, and 10,000 publishes. Assert identical ordered rows; a cursor
held on generation G never sees G+1; slot reuse waits for the guard; rollback of
an ordinary-table transaction exposes no partial frame. Count Rust global
allocator events, SQLite `sqlite3_status64` memory/high-water values, bytes
copied for blob columns, prepare/step time, WAL bytes, and busy results. Run
status: **proposed, not executed**.

## Unknowns

- Whether rusqlite `Context::set_result` selects transient copying for borrowed
  strings/blobs at 0.40.2; audit the exact implementation before claiming zero-copy.
- Safe registration/client-data ownership for an `Arc<FrameRing<_>>` and cursor
  lifetimes under connection close, statement reset, panic, and re-entrant SQL.
- Query planner quality for the custom costs and whether joins materialize or
  sort virtual rows despite `orderByConsumed`.
