# hafley-observe

Shared tracing configuration for hafley-rs binaries. `HAFLEY_OTLP_ENDPOINT`
turns on OTLP/HTTP span export alongside the formatter; unset, the process
keeps the formatter only. End every `init` caller's `main` with
`hafley_observe::shutdown()` to flush the last batch.

Run the local DuckDB viewer, then run a binary with the endpoint set:

    otel-desktop-viewer --db /tmp/observe.duckdb --open-browser=false
    HAFLEY_OTLP_ENDPOINT=http://127.0.0.1:4318/v1/traces ./your-binary

The DuckDB file stays locked while the viewer runs. Stop it, then query:

    duckdb -readonly /tmp/observe.duckdb "SELECT name, (end_time-start_time)/1000 dur_us FROM spans ORDER BY start_time"

## Test attributes

Alias this package as `oh`, import the attribute, and write ordinary test
functions. The macro emits Rust's built-in test attribute and runs the body
inside a per-test tracing dispatcher.

```toml
oh = { package = "hafley-observe", path = "../hafley-observe" }
```

```rust
use oh::test;

#[test(time_ms = 5000, logs = 100)]
fn parses_fixture() {
    tracing::info!("fixture loaded");
}
```

`logs` is a maximum event count at any one tracing callsite. Memory budgets
require installing `oh::counting_allocator!()` as the test binary's global
allocator. `instrument_all` accepts inline modules and `impl` blocks, and
`#[oh::skip]` excludes a function from that pass. The `default` feature set is
empty; binaries select their subscriber layers explicitly.
