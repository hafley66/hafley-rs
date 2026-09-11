#!/bin/sh
set -eu
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
if [ "${1:-}" != "--skip-build" ]; then
  lab_cargo build --features gdext --bin pigeon-rollback
fi
run_dir=$(lab_temp perf)
cd "$run_dir"
RUST_LOG='warn,pigeon::runtime=debug,pigeon::rollback=debug,pigeon::sql=trace,pigeon::snapshot=trace,pigeon::physics=trace,pigeon::verification=trace' \
  "$lab_dir/target/debug/pigeon-rollback" --sql --verify-only >protocol.log 2>tracing.jsonl
rg 'SQL_BOUNDARY_OK' tracing.jsonl
python3 "$lab_dir/66_trace_report.py" "$run_dir/tracing.jsonl" >report.md
cat report.md
RUST_LOG=warn "$lab_dir/target/debug/pigeon-rollback" --launch --verify-only \
  >golden-protocol.log 2>golden-diagnostics.log
jq -S '[.[][] | .world]' "$lab_dir/17_launch_trace.json" >expected-worlds.json
jq -S '[.[][] | .world]' 17_launch_trace.json >actual-worlds.json
cmp expected-worlds.json actual-worlds.json
printf 'GOLDEN_OK 360 authoritative worlds match\n'
