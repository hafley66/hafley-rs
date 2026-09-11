# Release runtime measurements

Run `sh 71_run_release.sh`. After a successful current-source release build,
`sh 71_run_release.sh --skip-build` repeats just the measurement. The script
prints a compact summary and leaves raw per-tick nanoseconds and decoder
diagnostics in its printed temporary directory.

## Measurement boundaries

The existing CLI's `--measure-release` mode requires a release build and runs
before subscriber initialization. No tracing subscriber is installed; the script
also sets `RUST_LOG=off`. Existing instrumentation's disabled checks remain in
the binary. No collector or telemetry output is active.

Each sample measures `Runtime::advance(input) -> Result<[Display; 2], Error>`
with `std::time::Instant`. It includes both GGRS peers, snapshot clones, checksums,
Rapier steps, rollback work, and presentation-row encoding. It excludes SQLite
publication/queries, Godot/wgpu rendering, disk output, initialization, asset
decoding, and full-state golden comparisons. Returned Display values are retained
until verification, so their subsequent destruction is outside the timed region.
Drops internal to advance are included. Timer and black_box overhead are included.

There are three warmup scenarios and ten measured scenarios, each with a newly
initialized Runtime and the same 180 inputs. Actions and the golden fixture are
loaded once. Each scenario retains its 180 output pairs, then compares all 360
authoritative states with the committed golden fixture outside advance timings.
The verification phase is timed separately. All 13 scenarios are checked.

Tick 97 is reported separately: delayed packets arrive and peer B executes 20
advances including replay. The report asserts that restore occurred and checks
that count. Other ticks include idle, jump, hit, flight, and ground contact;
their distribution mixes these workloads. The per-scenario runtime total sums
the 180 call durations, excluding loop and harness work between calls.

Statistics use nearest-rank median and p95. Raw samples are retained so other
aggregations can be computed without rerunning. Ten delivery samples do not
establish a stable tail-latency bound. The host is not CPU-isolated. These are
wall times for this fixture, not CPU self time, maximum throughput, allocation
counts, memory bandwidth, or a frame-budget guarantee. No before-optimization
release baseline was collected; debug traced results are a different configuration.

## Executed results

On the Apple M2 Pro host, the release build completed in 3m 33s. The following
run used no concurrent build from this task. `72_release_measurements.json`
contains its summary and all 1,800 measured tick durations.

| Timed region | Mean | Median | p95 | Maximum |
| --- | ---: | ---: | ---: | ---: |
| Ordinary tick, both peers (1,790 calls) | 30.43 µs | 29.54 µs | 43.75 µs | 83.29 µs |
| Delivery tick 97, both peers (10 calls) | 325.82 µs | 315.50 µs | 381.71 µs | 381.71 µs |
| Sum of 180 advance calls (10 scenarios) | 5.773 ms | 5.814 ms | 6.040 ms | 6.040 ms |
| Separate full-state verification (10 scenarios) | 12.031 ms | 11.933 ms | 13.054 ms | 13.054 ms |

All 4,680 authoritative state comparisons passed, including warmup scenarios.
Every scenario asserted the tick-97 restore and 20 peer-B advances. The full
runtime remains unchanged by this measurement mode. No new MP4 was recorded.
The gdext-enabled debug library suite passed all 15 tests in 12.08s, including
the new nearest-rank statistics/unit test and existing rollback/tracing tests.
