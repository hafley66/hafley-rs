#!/bin/sh
set -eu
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ "${1:-}" != "--skip-build" ]; then
  env -u CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER cargo build \
  --manifest-path "$lab_dir/Cargo.toml" -j 2 --locked --offline \
  --features gdext --bin falcon-rollback
fi
run_dir=$(mktemp -d /private/tmp/falcon-perf.XXXXXX)
cd "$run_dir"
RUST_LOG='warn,falcon::runtime=debug,falcon::rollback=debug,falcon::sql=trace,falcon::snapshot=trace,falcon::physics=trace,falcon::verification=trace' \
  "$lab_dir/target/debug/falcon-rollback" --sql --verify-only >protocol.log 2>tracing.jsonl
rg 'SQL_BOUNDARY_OK' tracing.jsonl
python3 "$lab_dir/66_trace_report.py" "$run_dir/tracing.jsonl" >report.md
cat report.md
RUST_LOG=warn "$lab_dir/target/debug/falcon-rollback" --launch --verify-only \
  >golden-protocol.log 2>golden-diagnostics.log
jq -S '[.[][] | .world]' "$lab_dir/17_launch_trace.json" >expected-worlds.json
jq -S '[.[][] | .world]' 17_launch_trace.json >actual-worlds.json
cmp expected-worlds.json actual-worlds.json
printf 'GOLDEN_OK 360 authoritative worlds match\n'
