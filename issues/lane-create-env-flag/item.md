---
created: 2026-09-02
updated: 2026-09-02
type: feature
reporter: hafley66@gmail.com
status: open
priority: normal
related: ['@boop-probe-collision']
---

# boop lane create: --env KEY=VAL flag

## Description

Observed (2026-09-02, ascii-renderer perf-instrumentation lanes): lane briefs currently must carry CARGO_TARGET_DIR per lane because `boop lane create` has no env flag. Each brief has to embed an export line or shell wrapper to set per-lane env vars instead of the spawn command taking them directly.

Ask: add a repeatable `--env KEY=VAL` flag to `boop lane create` so the coordinator can set per-lane environment variables (e.g. CARGO_TARGET_DIR) at spawn time without baking them into the brief text.

Filed alongside @boop-probe-collision, which is the primary issue from the same lane run.
