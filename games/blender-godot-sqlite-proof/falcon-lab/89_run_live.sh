#!/bin/sh
set -eu
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ "${1:-}" != "--skip-build" ]; then
  env -u CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER cargo test \
    --manifest-path "$lab_dir/Cargo.toml" --locked --offline -j 2 --features gdext --lib -- --test-threads=1
  env -u CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER cargo build \
    --manifest-path "$lab_dir/Cargo.toml" --locked --offline -j 2 --features gdext --lib --bin falcon-rollback
fi
python3 "$lab_dir/75_udp_lab.test.py"
python3 "$lab_dir/84_live_controller.test.py"
godot --headless --path "$lab_dir/godot" --check-only --script res://2_stage.gd
run_dir=$(mktemp -d /private/tmp/falcon-live-suite.XXXXXX)
mkdir "$run_dir/basic" "$run_dir/lifecycle"
cd "$run_dir/basic"
python3 "$lab_dir/84_live_controller.py"
python3 "$lab_dir/84_live_controller.py" --verify-archive
cd "$run_dir/lifecycle"
python3 "$lab_dir/84_live_controller.py" --lifecycle
python3 "$lab_dir/84_live_controller.py" --verify-archive
cd "$run_dir"
ffmpeg -v error -i basic/godot-0.avi -an -c:v libx264 -threads 2 -preset veryfast -crf 20 \
  -pix_fmt yuv420p -movflags +faststart live.mp4
ffmpeg -v error -i lifecycle/godot-0.avi -i lifecycle/godot-1.avi \
  -filter_complex '[0:v][1:v]concat=n=2:v=1:a=0[v]' -map '[v]' -an -c:v libx264 \
  -threads 2 -preset veryfast -crf 20 -pix_fmt yuv420p -movflags +faststart restart.mp4
ffmpeg -v error -i lifecycle/godot-2.avi -an -c:v libx264 -threads 2 -preset veryfast -crf 20 \
  -pix_fmt yuv420p -movflags +faststart cold.mp4
for clip in live restart cold; do
  ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,nb_frames,duration \
    -of json "$clip.mp4" >"$clip-media.json"
  jq -e '.streams[0] | .codec_name == "h264" and .width == 960 and .height == 540 and (.nb_frames|tonumber)>0' "$clip-media.json"
done
printf 'LIVE_SUITE_OK artifacts=%s\n' "$run_dir"
