# ryi SQLite writer, lane S3

## Crate and arena decisions

- `rustc-hash = "2"`: `rustc_hash::FxHasher` implements `std::hash::Hasher` and can fingerprint UTF-8 bytes without constructing a `DefaultHasher` per call. Version 2.1.3 was already present in `crates/sprefa-extract/Cargo.lock` transitively, so the direct dependency adds no package version. Fingerprints map to a vector of spans; byte equality decides reuse, including when fingerprints collide. Interning stays batch-local and strings over 128 bytes bypass it.
- `bumpalo = "3"`: `Bump::alloc_slice_copy(&[u8])` returns a contiguous `&mut [u8]`, so SQLite's virtual table can read each value as a byte span. `Val::Text` stores that span's pointer and length; the batch owns the `Bump` and resets it only after the virtual-table drain has returned. `Bump::reset()` resets the current chunk and releases earlier chunks, retaining the current chunk for batch recycling. `bumpalo::collections::Vec<u8>` implements `std::io::Write`; `into_bump_slice()` returns the final contiguous slice, allowing `serde_json::to_writer` to keep JSON serialization in the bump.
- The existing `HashMap` remains the lookup structure. Hash collisions retain distinct spans in the hash bucket and compare bytes before reuse.

## Spans and fields

- `sqlite_table_batch_drain`: one span per table batch drain; fields `table` and `rows`. Its lifetime covers the drain.
- `sqlite_bind_phase`: one debug span held across the export, entered for each bind call; fields `seconds` and `dispatch_meta_lookup_seconds`. Close-time `sqlite_bind_column_kind` events report `kind`, `calls`, `nulls`, and `seconds` for each `COLUMN_KINDS` entry. Close-time `sqlite_table_profile` events report `table`, `rows`, `bind_seconds`, and `insert_seconds`.
- `sqlite_export_total`: created with the database and held through close; entered around bind and pending-row writes and again through close; fields `rows` and `seconds` are recorded at close.
- `RYI_SQLITE_PHASES` was removed. Per-row and per-column clocks are enabled only when `tracing::enabled!(Level::DEBUG)` is true, computed once in `Binder::new`. Set `RUST_LOG=sprefa_extract=debug,hafley_scm=info` to capture detailed SQLite bind metrics through hafley-observe. No environment variable was added.

## Verification status

- All performance and behavior expectations from the S and S2 plans remain **unverified** for this change. No Cargo command or `ryi` run was performed.
- The existing interner collision test was extended to check repeated reuse within a forced collision bucket and the greater-than-128-byte bypass. Recycle and JSON-span assertions were adapted for `Bump`; these tests are **unverified** and were not run.
- SQLite row parity, output bytes, span output, total timings, memory use, and performance effects are **unverified**.
