#!/bin/sh
set -eu
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ "${1:-}" != "--skip-build" ]; then
  env -u CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER cargo build \
    --manifest-path "$lab_dir/Cargo.toml" --release --locked --offline \
    -j 2 --features gdext --bin falcon-rollback
fi
run_dir=$(mktemp -d /private/tmp/falcon-release.XXXXXX)
cd "$run_dir"
RUST_LOG=off "$lab_dir/target/release/falcon-rollback" --measure-release \
  >measurements.json 2>diagnostics.log
jq 'del(.raw_tick_nanoseconds_by_run)' measurements.json
printf 'Artifacts: %s\n' "$run_dir"
