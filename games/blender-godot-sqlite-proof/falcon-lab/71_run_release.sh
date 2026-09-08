#!/bin/sh
set -eu
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
if [ "${1:-}" != "--skip-build" ]; then
  lab_cargo build --release --features gdext --bin falcon-rollback
fi
run_dir=$(lab_temp release)
cd "$run_dir"
RUST_LOG=off "$lab_dir/target/release/falcon-rollback" --measure-release \
  >measurements.json 2>diagnostics.log
jq 'del(.raw_tick_nanoseconds_by_run)' measurements.json
printf 'Artifacts: %s\n' "$run_dir"
