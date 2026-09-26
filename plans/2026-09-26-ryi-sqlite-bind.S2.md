# ryi SQLite bind, lane S2

Base `f2740bfd`. Scope: `0_sqlite.rs`, `0a_bind.rs`. Gates deferred by instruction.

## Changes

- Typed rows continue to serialize directly into the vtab's columnar `Batch`; no `serde_json::Value` is constructed on that path.
- Schema setup builds column indexes and caches metadata column positions, replacing per-field linear scans.
- Each batch interns short repeated UTF-8 text by hash and verifies bytes before reusing a span. Clearing a recycled batch retains its value, text, and interner capacity. Hash collisions preserve distinct text.
- JSON columns write into the batch byte arena through `serde_json::to_writer`, avoiding a temporary `String`. The vtab and prepared-VALUES fallback bind those UTF-8 spans as SQL text.
- Per-row bind timing runs only with `RYI_SQLITE_PHASES`; phase reporting remains available.
- Written, unrun test rows: repeated text shares a span across a batch and clears on recycle; JSON bytes match `serde_json::to_string` for Unicode and escapes.

## Expected effect and measurement

| Table | Baseline bind, s | Expected effect | Measure |
| --- | ---: | --- | --- |
| `edge` | 0.829 | Cached column positions and repeated path, ID, and text spans reduce bind work | bind seconds, arena bytes, rows |
| `node` | 0.641 | Indexed fields and repeated text spans reduce bind work | bind seconds, arena bytes, rows |
| `df_lit` | 0.067 | Indexed fields reduce dispatch; JSON writer applies if JSON columns occur | bind seconds, rows |
| Other tables | 0.206 | Indexed fields and repeated text spans where present | per-table bind seconds, rows |

The 2,000-TS-file baseline was export 2.239 s, bind 1.743 s, insert 1.121 s. Re-run the same corpus with `RYI_SQLITE_PHASES=1`; compare total and per-table bind, insert, export wall, peak RSS, and database bytes. Compare every table's rows in both `EXCEPT` directions against the S writer and run the frozen ratchet ladder. No new measurements or output-parity claim are made here.

Bind still runs in the output callback, while the existing writer thread inserts. Moving it onto extraction workers requires changing the callback and worker handoff in `ryi.rs` and the extractor, outside this lane's two-file scope.
