# hafley-observe

## Allocation and span-close resource records

The `counting_allocator!()` macro installs one system-backed global allocator
from [`tracking-allocator` 0.4](https://docs.rs/tracking-allocator/0.4.0/tracking_allocator/).
`oh::test` uses its live and peak counters for memory budgets. The `span-alloc`
feature adds [`AllocationLayer`](https://docs.rs/tracking-allocator/0.4.0/tracking_allocator/struct.AllocationLayer.html)
and allocation groups. Call `allocation::tracked(span)` before entering a span
to count requested allocation bytes in that span. The innermost active group
owns each allocation.

With `rusage` enabled, set `HAFLEY_RUSAGE_SPANS` to a comma-separated list of
span names. Each selected span closes with `cpu.thread_ns`,
`mem.rss_end_bytes`, `mem.alloc_end_bytes`, and `mem.span_alloc_bytes` in its
resource record. `mem.alloc_end_bytes` is available when the allocator is
installed and tracking is enabled.

For the CLI, build `ryii` with `--features cli,read,profile-alloc` to enable
the allocator and per-span groups. The feature stays off in the default build
because the paired measurements put the wall cost well over 2%: the slow graph
on hafley-rs +4.65% and on tokio +31.88%
(`plans/2026-09-29-slow-on-demand-timing.tsv`), and `ryii fast` on hafley-rs
+49.73% (`plans/2026-09-29-fast-alloc-overhead.tsv`).

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
