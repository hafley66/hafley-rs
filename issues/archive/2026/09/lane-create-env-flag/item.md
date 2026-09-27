---
created: 2026-09-02
updated: 2026-09-05
type: feature
reporter: hafley66@gmail.com
status: done
priority: normal
related: ['@boop-probe-collision']
closed: 2026-09-05
---

# boop lane create: --env KEY=VAL flag

## Description

Observed (2026-09-02, ascii-renderer perf-instrumentation lanes): lane briefs currently must carry CARGO_TARGET_DIR per lane because `boop lane create` has no env flag. Each brief has to embed an export line or shell wrapper to set per-lane env vars instead of the spawn command taking them directly.

Ask: add a repeatable `--env KEY=VAL` flag to `boop lane create` so the coordinator can set per-lane environment variables (e.g. CARGO_TARGET_DIR) at spawn time without baking them into the brief text.

Filed alongside @boop-probe-collision, which is the primary issue from the same lane run.

## Agent Runs

### 2026-09-05T23:59:18Z · @flash4-lane

Tests Run:
- cargo test -p boop: env_pairs_ride_the_dry_run_cmd_line, a_value_without_equals_is_refused_at_the_cli, a_key_colliding_with_a_boop_stamp_is_refused_by_name all pass.
- dry-run cmd: line carries A='1' B='two words' after the boop stamps.
- clippy: pre-existing baseline errors (run_host dead-code in debug.rs, unnecessary-unwrap in job.rs) unrelated to --env.
