# ryi SQLite writer, lane S3

## Crate and arena decisions

| Candidate | API and dependency facts | Choice |
| --- | --- | --- |
| `rustc-hash` | `rustc_hash::FxHasher` implements `std::hash::Hasher`; version 2.1.3 was already in `crates/sprefa-extract/Cargo.lock` transitively. | Selected for one fast fingerprint per short string. |
| `foldhash` | Also supplies a fast hash API, but was not a direct dependency; hashbrown already depends on foldhash internally. | Not selected, since adding an independent hash API does not provide the prehashed table lookup used here. |
| `hashbrown::HashTable<Val>` | Version 0.16.1 was already in the crate lock. `find(hash, eq)` accepts the caller's `u64` hash and byte-equality closure; `insert_unique(hash, value, hasher)` accepts the same precomputed hash and calls `hasher` for existing entries when the table grows. The closure recomputes Fx over each stored span's bytes. This avoids a `HashMap` second hash and a `Vec` allocation per unique hash bucket. | Selected. Each batch owns one table; equal byte strings reuse a `Val`, and hash collisions compare the referenced bytes before reusing. |
| `lasso::Rodeo` / `ThreadedRodeo` | Its public lookup API returns a `Spur`; `resolve(&Spur)` returns a borrowed `&str`, not a `(u32, u32)` byte offset. The crate was absent from this lock. | Not selected; `hashbrown::HashTable` accepts the already computed hash and preserves the chosen offset representation. |
| `bumpalo::Bump` | `Bump::alloc_slice_copy(&[u8])` returns a contiguous `&mut [u8]`, and `bumpalo::collections::Vec::into_bump_slice()` returns a contiguous slice for JSON output. Pointer-plus-length `Val::Text` therefore works with SQLite, but occupies 24 bytes in the current enum layout. | Not selected. `Vec<u8>` keeps `Val::Text(u32, u32)` offsets and `size_of::<Val>() == 16`; offsets remain valid through each batch drain and `Vec::clear()` retains capacity. |

Interning is per batch. Strings longer than 128 bytes bypass the table. JSON serialization appends to the byte vector and truncates to its starting offset if serialization returns an error.

## Spans and fields

- `sqlite_table_batch_drain`: one info span per table batch drain, fields `table` and `rows`, covering the drain call.
- `sqlite_bind_phase`: debug child span; `seconds` and `dispatch_meta_lookup_seconds` are recorded once from accumulated bind timing at close. It is not entered per row. Close-time `sqlite bind column kind` events iterate `COLUMN_KINDS` and report `kind`, `calls`, `nulls`, and `seconds`. `sqlite table profile` events report `table`, `rows`, `bind_seconds`, and `insert_seconds`.
- `sqlite_export_total`: created and entered once when the database is created; its `EnteredSpan` is held through close. Row writes and pending-row flushes inherit that context without per-row span clones or enter/exit calls. Fields `rows` and `seconds` are recorded at close.
- `RYI_SQLITE_PHASES` and its `eprintln!` timings were removed. Per-row and per-column clocks are enabled only when `tracing::enabled!(Level::DEBUG)` is true, cached once in `Binder::new`. Set `RUST_LOG=sprefa_extract=debug,hafley_scm=info` to capture detailed metrics through hafley-observe. No environment variable was added.

## Verification and measurement

- All performance and behavior expectations remain **unverified**. No Cargo command, test command, or `ryi` run was performed.
- Run this measurement against a 2,000-file TypeScript corpus, substituting concrete corpus and output database paths:

  ```sh
  RUST_LOG=sprefa_extract=debug,hafley_scm=info ryi fast <2000-file-TS-corpus> --sqlite <output.sqlite>
  ```

- Compare the emitted export, bind, and insert timings with the S2 baseline: export 2.239 s, bind 1.743 s, insert 1.121 s. Those timings and the comparison are **unverified** for S3.
- `val_stays_sixteen_bytes`, the collision/bypass and 1,000-entry resize tests, recycled-capacity assertions, JSON offset assertions, SQLite row parity, output bytes, span output, memory use, and performance effects are **unverified**.
- The replacement for the S2 note `RYI_SQLITE_PHASES=1` is `RUST_LOG=sprefa_extract=debug,hafley_scm=info`; S2.md was not edited because it belongs to another lane.
