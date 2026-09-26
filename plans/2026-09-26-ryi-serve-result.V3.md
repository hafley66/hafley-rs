# ryi V3: Result exits and serve contract

Base `96292970`.

- `RyiExit { code, message }` carries handler stops through the in-process transport. CLI maps the code to process exit; HTTP maps it to a status and a JSON error row. Removed `TransportExit`, the panic hook, and `catch_unwind`.
- `serve(listen: string)` is in `schema/cli/ops.tsp`; checked-in clap now parses it and root help lists it.
- Exact added root-help line: `  serve   Serve the HTTP contract`.
- 178 adds an invalid SCIP indexer request: HTTP 400, JSON error row with code 2, incomplete row, then a successful `/schema` request on the same server. Serve help also has a direct assertion. A unit row covers an error after one data row.
- `just gen-cli` was attempted and stopped before TypeSpec compilation: `/Users/chrishafley/projects/hafley-tsp/packages/rust/dist/emitter/06_on-emit.js` is absent. The emitter needs a build, which this lane forbids. Generated Rust was edited directly; regeneration and generated-contract parity remain for the coordinator after that build.
- JSONL responses spool to an anonymous temporary file before headers, so errors after data rows also get a non-200 status. Memory stays bounded; first-byte latency now includes the operation runtime.
- No cargo build, test, ryi, or CodeQL command ran in this lane.
