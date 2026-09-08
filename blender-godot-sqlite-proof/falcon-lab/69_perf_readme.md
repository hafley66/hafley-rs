# Headless span timing pass

Run `sh 67_run_perf.sh` from this directory, or pass `--skip-build` after a
successful build of the current source. Uses existing Rust tracing-subscriber
JSON, Python standard library, rg, and jq. No collector or service is installed.
The script leaves raw traces, a ranked report, and golden comparison files in
the printed temporary directory. `python3 66_trace_report.test.py` tests units,
diagnostic filtering, aggregation, and delivery-tick grouping.

## Measured change

`68_perf_measurements.md` records the original JSON-cloning baseline followed
by two direct-component-cloning runs. Same debug binary configuration, fixture,
and tracing filters; no test build was running during these selected passes.
An earlier changed pass overlapped a build and is excluded from comparison.
The laptop was not otherwise isolated. This is a diagnostic comparison, without
statistical confidence intervals or release-performance claims.

| Inclusive busy total | Before | After 1 | After 2 |
| --- | ---: | ---: | ---: |
| Physics clone, 2,564 calls | 2,736.096 ms | 35.375 ms | 33.964 ms |
| Runtime advance, 540 calls | 2,983.350 ms | 399.692 ms | 394.511 ms |
| Physics equality, 803 calls | 937.030 ms | 864.926 ms | 845.204 ms |

Nested totals overlap. JSON output has synchronous I/O and instrumentation costs.
Span busy time includes descheduling while entered; it is not CPU self time.
The verifier runs three 180-tick scenarios, so the tick-97 bucket mixes delivery
and clean-reference scenarios. Individual replayed steps are not separated by
this report. Allocation counts, memory bandwidth, and peak RSS remain unmeasured.

`Sandbag::clone` now uses the existing Rapier 0.35.3 component Clone APIs.
`PhysicsWorld` itself has no Clone implementation. Its physics pipeline and CCD
solver are serde-skipped workspaces and are recreated. Component clones can
retain internal scratch state that serde skips. Fixture equivalence is checked;
arbitrary-world equivalence is not claimed. JSON equality remains unchanged and
is now the largest recorded span total in this verifier.

## Verification

The modified CLI built successfully and completed the SQL verifier: 180
generations, maximum 450 rows, three slots, held damage 0 and corrected damage 18.
A separate `--launch --verify-only` run matched all 360 authoritative World JSON
objects against the committed `17_launch_trace.json`, including Rapier state.
Whole Display objects have additional fields introduced since that golden was
recorded, so comparison deliberately selects `.world`.

The replay regression test now checks checkpoints 0, 91, 105, 128, and 179,
including snapshot isolation and each remaining step against legacy JSON clones.
Initial Cargo test builds were terminated with SIGTERM, including an escalated
attempt. On the subsequent clean-build retry, `cargo clean` removed only this
lab's target artifacts (5.6 GiB). The two-job, locked, offline, gdext-enabled
library build completed in 1m 43s, and all 14 tests passed in 12.44s, including
the expanded replay assertions. Two Python report tests passed in the original
pass. Existing golden and CLI assertions also executed successfully.

No rendering code changed and no new MP4 was recorded in this headless pass.
