#!/bin/sh
set -eu
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ "${1:-}" != "--skip-build" ]; then
  env -u CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER cargo build \
    --manifest-path "$lab_dir/Cargo.toml" --release --locked --offline \
    -j 2 --features gdext --bin falcon-rollback
fi
python3 "$lab_dir/75_udp_lab.test.py"
run_dir=$(mktemp -d /private/tmp/falcon-processes.XXXXXX)
cd "$run_dir"
python3 "$lab_dir/75_udp_lab.py" "$lab_dir/target/release/falcon-rollback"
"$lab_dir/target/release/falcon-rollback" --process-video
ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,nb_frames,duration \
  -of json processes.mp4
cat verification.json
printf '\nArtifacts: %s\n' "$run_dir"
