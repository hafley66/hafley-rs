# Names load baseline

One run on feature/ryi-ra-names-fast, parent c2299b19, macOS arm64.
`KACHE_DISABLED=1 cargo test -p hafley_scm --features rust-checker cold_and_warm_names_hosts -- --ignored --nocapture`.

| workspace | state | host_open_seconds | module_query_seconds | module_places |
| --- | --- | ---: | ---: | ---: |
| hafley-rs | cold | 0.200086 | 0.481328 | 1 |
| hafley-rs | warm | 0.042252 | 0.000063 | 1 |
| sprefa-extract | cold | 0.282301 | 0.552348 | 1 |
| sprefa-extract | warm | 0.051235 | 0.000075 | 1 |

Each workspace opens twice in one process. This exercises the process-wide host
cache that daemon requests share, without HTTP transport. The warm-in-daemon HTTP
measurement remains outstanding. Host open includes Cargo discovery and metadata;
the first query builds the lazy def maps. These are Names loads with no sysroot or
inference. Compilation (5m18s) is outside the measured intervals.

The ignored test is the reproducible measurement entry point. It must be invoked
explicitly and is excluded from ordinary gates. No whole-corpus comparison ran.
