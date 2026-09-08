#!/bin/sh
set -eu
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
if [ "${1:-}" != "--skip-build" ]; then
  lab_cargo build --release --features gdext --bin falcon-rollback
fi
python3 "$lab_dir/75_udp_lab.test.py"
run_dir=$(lab_temp processes)
cd "$run_dir"
python3 "$lab_dir/75_udp_lab.py" "$lab_dir/target/release/falcon-rollback"
"$lab_dir/target/release/falcon-rollback" --process-video
lab_probe processes.mp4
cat verification.json
printf '\nArtifacts: %s\n' "$run_dir"
