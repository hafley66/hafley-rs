# ryi V3: Result exits and serve contract

Base `96292970`.

- `RyiExit { code, message }` carries handler stops through the in-process transport. CLI maps the code to process exit; HTTP maps it to a status and a JSON error row. Removed `TransportExit`, the panic hook, and `catch_unwind`.
- `serve(listen: string)` is in `schema/cli/ops.tsp`; checked-in clap now parses it and root help lists it.
- Exact added root-help line: `  serve   Serve the HTTP contract`.
- 178 adds invalid SCIP indexer CLI exit 2 and HTTP 400 with a JSON error and incomplete row; a query that emits a row before failing keeps HTTP 200 and ends with a JSON error; `/schema` then succeeds on the same server. Serve help has a direct assertion. A unit row proves headers arrive before finite producer completion.
- `just gen-cli` stopped before TypeSpec compilation: `/Users/chrishafley/projects/hafley-tsp/packages/rust/dist/emitter/06_on-emit.js` is absent; building the emitter is outside this lane. Coordinator will build it and run `just gen-cli` at merge.
- Hand-edited `gen/cli_auto.rs` for `Cmd::Serve` and its operation guard; `gen/ops_auto.rs` for `ServeArgs` and coded `OpError`; `gen/http_auto.rs` for coded HTTP errors, live JSONL rows, and watch cancellation. Regeneration must preserve these behaviors.
- Finite JSONL responses stream over a bounded channel as rows arrive. An error before the first data row sets a non-200 status; a later error is the final JSON row with HTTP 200.
- No cargo build, test, ryi, or CodeQL command ran in this lane.

## Review follow-up

- Removed the process-wide stdout redirect, gate, and pipe reader. Each operation owns a bounded row sink; edit text is routed through its operation thread's sink.
- Persistent `/watch` returns a live body. Dropping it sets cancellation; the idle watch loop checks it within 250 ms, and a closed receiver makes writes fail with `BrokenPipe`.
- SQLite completion text becomes JSON string rows for streaming HTTP operations. A 178 row checks `/fast?sqlite=...` succeeds and publishes its database.
- Default `fast` JSONL uses `diet_scip_streamed` and an on-disk SQLite BINARY sort. A 178 row exercises 4,097 files, checks sorted bytes, and checks the retained `sorted_lines` trace stage is absent.
- 178 also checks an open watch permits a query, accepts a live row, and leaves the server responsive after disconnect; a unit row checks cancellation and closed-sink behavior.
- A watch failure after its first data row retains the sent HTTP 200 and ends with a JSON error row; HTTP status cannot change after live headers are sent.
- These follow-up rows were written and not run under the lane's no-build, no-test rule.
