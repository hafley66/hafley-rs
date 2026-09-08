#!/bin/sh
set -eu
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
if [ "${1:-}" != "--skip-build" ]; then
  lab_cargo test --features gdext --lib -- --test-threads=1
  lab_cargo build --features gdext --lib --bin falcon-rollback
fi
python3 "$lab_dir/75_udp_lab.test.py"
python3 "$lab_dir/84_live_controller.test.py"
godot --headless --path "$lab_dir/godot" --check-only --script res://2_stage.gd
godot --headless --path "$lab_dir/godot" --script res://1_rows_test.gd
run_dir=$(lab_temp live-suite)
mkdir "$run_dir/basic" "$run_dir/lifecycle"
cd "$run_dir/basic"
python3 "$lab_dir/84_live_controller.py"
python3 "$lab_dir/84_live_controller.py" --verify-archive
cd "$run_dir/lifecycle"
python3 "$lab_dir/84_live_controller.py" --lifecycle
python3 "$lab_dir/84_live_controller.py" --verify-archive
cd "$run_dir"
lab_encode basic/godot-0.avi live.mp4
ffmpeg -v error -i lifecycle/godot-0.avi -i lifecycle/godot-1.avi \
  -filter_complex '[0:v][1:v]concat=n=2:v=1:a=0[v]' -map '[v]' -an -c:v libx264 \
  -threads 2 -preset veryfast -crf 20 -pix_fmt yuv420p -movflags +faststart restart.mp4
lab_encode lifecycle/godot-2.avi cold.mp4
for clip in live restart cold; do
  lab_probe "$clip.mp4" >"$clip-media.json"
  jq -e '.streams[0] | .codec_name == "h264" and .width == 960 and .height == 540 and (.nb_frames|tonumber)>0' "$clip-media.json"
done
printf 'LIVE_SUITE_OK artifacts=%s\n' "$run_dir"
