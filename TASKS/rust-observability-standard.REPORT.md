# Rust observability standard and local project mapping

Date: 2026-08-24

## Short answer

The mainstream Rust application stack is:

1. `tracing` in libraries for structured spans and events.
2. `tracing-subscriber` in binaries for filtering, formatting, and output routing.
3. `metrics` for counters, gauges, and histograms.
4. OpenTelemetry as an optional export layer attached by the executable.
5. `console-subscriber` as an optional Tokio task debugger.
6. `thiserror` for typed library errors and commonly `anyhow` for executable error context.

The ownership boundary is explicit in the [`tracing` crate documentation](https://docs.rs/tracing/latest/tracing/): libraries emit spans and events, while executables install the subscriber. Libraries should not install a global subscriber.

## Instrumentation model

| Concern | Mainstream crate or mechanism | Owner | Data shape |
|---|---|---|---|
| Operation lifetime and causality | `tracing` spans | Library | Named span plus typed fields |
| Discrete diagnostic event | `tracing` events | Library | Level, message, typed fields |
| Filtering and sinks | `tracing-subscriber` | Executable | Layer stack and `EnvFilter` |
| Aggregate process measurements | `metrics` | Library records, executable exports | Counter, gauge, histogram |
| Distributed trace export | OpenTelemetry tracing layer | Executable | OTLP spans and resource attributes |
| Tokio task inspection | `console-subscriber` | Executable, debug mode | Task lifecycle and async resources |
| Durable audit or domain history | Application append-only record | Domain owner | Versioned structured schema |
| Returned failures | `Result` and typed errors | Function/API boundary | Error source chain and context |

`tracing-subscriber` supplies composable registries, filters, formatters, and layers. One executable can attach a human formatter, JSON formatter, OpenTelemetry exporter, and local aggregation layer to the same emitted spans and events. See [`tracing-subscriber`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/).

## Standard library and binary split

### Library crates

Library dependencies:

```toml
[dependencies]
tracing = "0.1"
metrics = "..." # only when the crate emits aggregate measurements
thiserror = "..." # only when the crate owns error types
```

Library behavior:

- Emit `tracing::info_span!`, `debug_span!`, `event!`, and level macros.
- Add fields with stable names and native values instead of interpolating them into messages.
- Keep span guards and `#[instrument]` scopes aligned with real operation lifetimes.
- Record metrics at the point where the counted side effect occurs.
- Return errors through `Result`.
- Leave subscriber installation, output destinations, and global filters to the executable.

### Executable crates

Executable dependencies:

```toml
[dependencies]
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt", "json"] }

# Feature-gated exporters and debugging:
tracing-opentelemetry = { version = "...", optional = true }
opentelemetry-otlp = { version = "...", optional = true }
console-subscriber = { version = "...", optional = true }
```

Executable behavior:

- Construct one `tracing_subscriber::Registry`.
- Parse `RUST_LOG` through `EnvFilter`.
- Select human stderr or JSON stderr formatting through configuration.
- Attach optional OTLP and Tokio console layers.
- Put command result data on stdout and diagnostics on stderr.
- Flush exporters before process exit.

## Fields

The common field vocabulary should cover identity, operation, cardinality, latency, and result:

```text
service.name
service.version
git.sha
process.pid
operation
component
repo.path
repo.git_common_dir
worktree.path
session.id
pane.id
harness.kind
query.name
rows.read
rows.written
items.scanned
items.changed
cache.hit
duration_ms
result
error.kind
error.message
```

The actual Rust field spelling can use underscores while exporters map stable fields to OpenTelemetry semantic conventions where applicable.

## SQL, contention, and full-scan visibility

Every database operation that can block or scale with table size needs one enclosing span and aggregate measurements:

```rust
let span = tracing::info_span!(
    "db.query",
    query.name = "materialize_native_sessions",
    db.system = "sqlite",
    db.path = %db_path.display(),
    rows.read = tracing::field::Empty,
    rows.written = tracing::field::Empty,
    duration_ms = tracing::field::Empty,
);
```

The completion path records:

- elapsed wall time;
- returned row count;
- scanned or visited item count when available;
- transaction and lock wait time when available;
- retry count and SQLite busy/locked result;
- cache hit or miss;
- caller operation and session identity.

Metrics provide repeated numeric detection:

```text
db_query_total{query,result}
db_query_duration_seconds{query}
db_rows_read_total{query}
db_rows_written_total{query}
db_lock_wait_seconds{query}
db_busy_total{query}
cache_access_total{cache,result}
process_spawn_total{program,result}
process_live{program,version}
```

An integration harness should assert bounded deltas for these counters. The assertion belongs at the external effect boundary so an N+1 regression changes a stable number. Wall-clock assertions remain secondary because scheduler and filesystem timing vary.

## OpenTelemetry status

OpenTelemetry Rust currently marks traces, metrics, and logs as Beta. Its Rust documentation says that OpenTelemetry does not provide an end-user logging API and documents bridging `tracing` events through `opentelemetry-appender-tracing`. See the [OpenTelemetry Rust status page](https://opentelemetry.io/docs/languages/rust/) and [Rust getting-started guide](https://opentelemetry.io/docs/languages/rust/getting-started/).

This supports keeping `tracing` as the application-facing instrumentation API. OTLP remains an executable-selected destination.

For Prometheus output, [`metrics-exporter-prometheus`](https://docs.rs/metrics-exporter-prometheus/latest/metrics_exporter_prometheus/) implements the `metrics` recorder and can expose a scrape endpoint or push to a gateway.

For Tokio runtime investigation, [`console-subscriber`](https://docs.rs/console-subscriber/latest/console_subscriber/) is a `tracing-subscriber` layer that exposes task and async-resource instrumentation to `tokio-console`. It requires the corresponding Tokio instrumentation configuration and belongs behind a debug feature or runtime flag.

## Current local project mapping

### `sprefa-extract`

Observed shape:

- Library uses `tracing`.
- CLI owns optional `tracing-subscriber` initialization.
- `RUST_LOG` controls the formatter layer.
- Default stderr is empty.
- `DL_TRACE_SUMMARY=1` adds a custom summary layer.
- Integration coverage asserts default stderr and summary output.
- No OTLP exporter or persistent structured log sink is installed.

Files:

- `sprefa-extract/src/trace.rs`
- `sprefa-extract/src/bin/extract.rs`
- `sprefa-extract/tests/31_tracing.rs`

This project already uses the mainstream library/executable ownership split. Its custom summary layer is compatible with the same registry used for formatting and future exporters.

### `soopy`

Observed shape:

- No `tracing`, `log`, or OpenTelemetry dependency.
- Typed `TrackedStateMetrics` records Git child processes, hash workers, byte workers, bytes hashed, and worktree cache hits and misses.
- `status-metrics` emits JSON containing cold/warm wall time, RSS, tracked metrics, peak RSS, and open file descriptors.
- Multi-repository refresh returns typed receipts.
- No subscriber, persistent diagnostic log, trace identifiers, or OTLP export.

The typed receipts and counters can remain the deterministic assertion surface. `tracing` spans and events can describe the causal timeline around those same operations, and `metrics` can export the repeated process-level aggregates.

### `boop`

Observed source counts from the prior inventory:

| Crate | `tracing` macros | `println!` | `eprintln!` |
|---|---:|---:|---:|
| `boop` | 32 | 98 | 5 |
| `boop-store` | 1 | 3 | 1 |
| `boop-proc` | 43 | 12 | 0 |
| `boop-acp` | 13 | 5 | 0 |
| `boop-harness` | 1 | 4 | 0 |
| `boop-mux` | 21 | 0 | 0 |
| `soopy` | 0 | 8 | 0 |

Observed tracing levels across crate source:

| Level | Count |
|---|---:|
| `debug` | 27 |
| `error` | 7 |
| `info` | 41 |
| `warn` | 36 |

Subscriber initialization was found in `boop/src/main.rs` and `boop-store/src/trail.rs` tests. Persistent outputs include:

- per-lane `supervise.log`;
- `child.stderr`;
- `sync-trail.ndjson`;
- `bus.ndjson`;
- `registry.json`;
- optional `BOOP_KNOWN_SESSIONS_TRAIL`.

The uniform shape is one executable-owned subscriber registry plus layers for stderr, durable NDJSON where required, summary aggregation, OTLP, and Tokio console. Versioned domain trails remain separate schemas even when their writes also emit tracing events.

## Shared contract

A common initialization API can be owned by a shared crate and called only by binaries:

```rust
pub struct ObservabilityConfig {
    pub service_name: &'static str,
    pub service_version: &'static str,
    pub git_sha: Option<&'static str>,
    pub format: LogFormat,
    pub filter: String,
    pub otlp_endpoint: Option<String>,
    pub tokio_console: bool,
}

pub struct ObservabilityGuard {
    // exporter flush guards and worker shutdown handles
}

pub fn init(config: ObservabilityConfig) -> Result<ObservabilityGuard, InitError>;
```

Instance timeline:

1. The binary parses environment and command-line configuration.
2. `init` builds one registry and attaches selected layers.
3. Libraries emit spans, events, and metrics without knowing the destinations.
4. The binary holds `ObservabilityGuard` for the process lifetime.
5. Drop or explicit shutdown flushes buffered exporters.

Storage and uniqueness:

- One global tracing subscriber per process.
- One metrics recorder per process.
- One stable service identity and build version per process.
- One trace or root operation identifier per command, daemon request, refresh cycle, or agent turn.
- Durable audit entries receive their own domain identifier and schema version.
- Test recorders are installed per test process or under serialized test control because both subscriber and recorder installation are global.

## Adoption order

1. Freeze the field vocabulary and executable configuration contract.
2. Put shared subscriber construction in one crate.
3. Preserve `sprefa-extract` default-silent behavior as a configuration preset.
4. Add `tracing` spans/events to Soopy around Git, filesystem, cache, worker, and refresh boundaries while retaining typed receipts.
5. Route Boop executable diagnostics through the shared registry.
6. Classify each `println!` and `eprintln!` as stdout result data, stderr user message, tracing event, or versioned durable trail.
7. Instrument SQLite query counts, row counts, lock waits, retries, and durations.
8. Add integration assertions for exact process spawn counts, query counts, cache accesses, and harness versions.
9. Add optional OTLP, Prometheus, and Tokio console layers without changing library instrumentation.
