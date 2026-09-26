# sol6 source review (REV)

| Severity | File:line | Commit | Defect | Concrete failure scenario |
| --- | --- | --- | --- | --- |
| high | `crates/sprefa-extract/src/bin/ryi/ops.rs:99` | `cd8ea5f7` | A persistent `watch` holds the process-wide stdout gate for its entire run. | `GET /watch` without `once` prevents later graph, query, diff, and edit requests from acquiring the gate. |
| high | `crates/sprefa-extract/src/bin/ryi/ops.rs:118` | `41b2fcb6` | The pipe reader ignores a closed response channel and leaves the producer running. | Disconnecting a watch client leaves its watcher and reader threads active indefinitely. |
| high | `crates/hafley_scm/src/read/lang/rust_modules.rs:597` | `5a4e12e2` | Cargo target boundaries collapse to one package directory for every file under `src/`. | A unique private `helper` in `src/bin/tool.rs` can become the resolved target of `helper()` in `src/lib.rs`. |
| high | `crates/hafley_scm/src/read/lang/rust/1_type.rs:201` | `7174f603` | Associated-type lookup uses a corpus-wide trait name without checking imports or crate visibility. | `use external::Trait; type T = Trait::Item` binds to an unrelated corpus crate's `Trait::Item`. |
| high | `crates/hafley_scm/src/read/project.rs:2343` | `5a4e12e2` | Cross-file target lookup still chooses the first path for a shared content blob. | Identical `a/shared.ts` and `b/shared.ts` make `b/main.ts`'s relative import report `a/shared.ts` as its call target. |
| high | `crates/sprefa-extract/src/bin/ryi.rs:616` | `5ec870fa` | The bounded fast path runs only for SQLite output; default JSONL still retains and sorts the full corpus. | `ryi fast` on a large root can exhaust the 2 GiB heap cap before emitting its first row. |
| med | `crates/hafley_scm/src/read/lang/rust_modules.rs:1297` | `119300f9` | The qualified type fallback searches modules by the final qualifier segment alone. | `crate::a::b::Widget` can resolve to the sole `Widget` in `crate::c::b`. |
| med | `crates/hafley_scm/src/read/lang/rust_modules.rs:1323` | `119300f9` | A missing qualified module is accepted when a same-name type is corpus-unique. | `crate::missing::Widget` resolves to a `Widget` declared elsewhere in the crate. |
| med | `crates/hafley_scm/src/read/lang/rust_modules.rs:674` | `5a4e12e2` | Normal, dev, and build path dependencies share one visibility set for every target. | `src/lib.rs` can bind to a type supplied only by a build dependency. |
| med | `crates/hafley_scm/src/read/lang/ts.rs:4695` | `11eb3096` | Same-file free calls are dropped when a peer export has the same name and arity, a branch described as retaining a mutation-battery rule. | A local `helper()` in `a.ts` becomes unresolved after `b.ts` exports an unrelated `helper` at another span. |
| med | `crates/sprefa-extract/src/edit/_6_rename.rs:181` | `ce2a7ecb` | Presence of `index.scip` switches a multi-row rename from sequential overlays to planning every row on original text. | A list `A -> B`, then `B -> C` succeeds without the index but fails to find `B` when the index exists. |
| med | `crates/sprefa-extract/src/bin/ryi/ops.rs:136` | `41b2fcb6` | Streaming HTTP operations parse every CLI output line as JSON, including SQLite completion text. | `POST /fast?sqlite=...` publishes the database, then returns a JSON parse error for its `Wrote ...` line. |
| med | `crates/hafley_scm/src/read/lang/rust_modules.rs:1200` | `8834c15f` | Fixture visibility depends on the literal absolute-path fragment `/tests/fixtures/`. | Relative `tests/fixtures/a` and `tests/fixtures/b` inputs bypass isolation and can bind each other's unique definitions. |
| low | `~/projects/hafley-tsp/packages/rust/src/emitter/04_ops-plan.ts:259` | `b12e6884` | Scalar HTTP headers discard invalid UTF-8 while array headers propagate the conversion error. | An optional header containing invalid UTF-8 silently becomes absent and receives its default. |

Counts: high 6, med 7, low 1.
Worst: a persistent `/watch` request holds the stdout gate and blocks every later stdout-capturing HTTP operation.
Test-gaming: yes; the TypeScript branch cites the mutation battery, and Rust fixture scoping keys on `/tests/fixtures/`.
