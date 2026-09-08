# Rust tracing standard and active instrumentation

`games/AGENTS.md` requires structured `tracing` spans/events at Rust execution
boundaries. The active Falcon package and renderer-free simulation core now
depend on `tracing` 0.1.44. The lab host uses `tracing-subscriber` 0.3.23 for JSON
output and environment filtering. Both crates are MIT licensed. Existing domain
dependencies remain unchanged; this uses the standard tracing ecosystem.

## Ownership and configuration

`0_tracing.rs` initializes the subscriber once at CLI/adapter startup, writes JSON
to stderr, and respects an existing host subscriber. Core library APIs never
initialize it. `RUST_LOG` controls filtering; the default is WARN. Span-close
events include subscriber-reported busy/idle duration when their level is enabled.
The synchronous JSON writer can itself add allocation and I/O overhead. No
allocation-free telemetry or throughput claim is made.

```sh
# Run from the Falcon lab. Store stderr outside the repository for inspection.
RUST_LOG='warn,falcon::runtime=debug,falcon::rollback=debug,falcon::sql=trace' \
  cargo run -j 2 --locked --offline --bin falcon-rollback -- --sql --verify-only

# For core snapshot/physics and presentation detail, add:
# falcon::snapshot=trace,falcon::physics=trace,falcon::presentation=trace
# Godot adapter: falcon::godot=trace
```

CLI verification writes its existing fixture output paths. Run from a temporary
working directory with an absolute `--manifest-path` to preserve saved evidence.
Use `env -u CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER` if the inherited linker
override is present. Godot gets the same RUST_LOG configuration through its
environment. Existing controller stdout markers remain compatible.

## Coverage

| Target | Work and scalar context |
| --- | --- |
| `falcon::runtime` | Advance call, tick, input bits |
| `falcon::rollback` | Peer, save/restore, checksum, advance/save/restore counts, confirmed tick |
| `falcon::simulation` | Authoritative step, tick, input bits |
| `falcon::snapshot` | Core save/load, tick, physics clone |
| `falcon::physics` | Launch parameters and physics step |
| `falcon::verification` | Physics equality serialization |
| `falcon::sql` | Publication generation/frame/row count, refusal reason, reader creation, frame query |
| `falcon::presentation` | Numeric row encoding and line geometry generation |
| `falcon::godot` | Poll, packed transfer, acknowledgement generation and vertex count |
| `falcon::worker` | Worker lifecycle and tick deadline events |

Workers explicitly retain the caller's Dispatch and parent Span. The worker
lifecycle span includes pacing waits and measures its lifetime; nested operation
spans measure individual synchronous work. Snapshot, asset, geometry, and World
payloads are excluded through `skip_all` and explicitly selected scalar fields.

`64_tracing_tests.rs` validates emitted JSON fields, parent hierarchy, publication
refusal diagnostics, state equality with tracing enabled, and scoped context
retention after the spawning caller returns. Existing deterministic tests remain
the correctness oracle. Whole-program allocation counts, allocated bytes, RSS,
copy bandwidth, and tracing overhead remain unmeasured. Historical Rust labs
outside this active path have not been retrofitted in this change.

API references: [instrument](https://docs.rs/tracing/0.1.44/tracing/attr.instrument.html),
[subscriber configuration](https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/fmt/struct.SubscriberBuilder.html).

## Executed verification

The 14-test gdext-enabled library executable passed, including both tracing
tests; all-target gdext Clippy passed with warnings denied. A Cargo test-driver
run was interrupted by SIGTERM, so the built test executable was run directly
to completion. The CLI verifier also completed with tracing enabled from a
temporary working directory, preserving committed fixtures. Its selected
runtime/rollback/SQL filters emitted 3,263 JSON records, including 2,003 timed
span-close events and two restore spans. The SQL verifier retained its 180
generations, 450-row maximum, and held-reader correction assertions. These are
instrumentation checks, not allocation or throughput benchmark results.
