#!/bin/sh
set -eu
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
node "$lab_dir/contracts/1_generate.mjs" --check
lab_cargo build --features gdext --lib
run_dir=$(lab_temp control)
godot --headless --path "$lab_dir/godot" --check-only --script res://2_stage.gd
godot --headless --path "$lab_dir/godot" --script res://1_payload.test.gd
godot --headless --path "$lab_dir/godot" --quit-after 120 --script res://3_control.test.gd -- --control >"$run_dir/keyboard.log" 2>&1
rg '^CONTROL_KEYBOARD_OK' "$run_dir/keyboard.log"
if [ "${1:-}" = "--test" ]; then
  exit 0
fi
if [ "${1:-}" = "--play" ]; then
  exec godot --path "$lab_dir/godot" -- --control
fi
export FALCON_CONTROL_PROOF="$run_dir/control-proof.json"
godot --path "$lab_dir/godot" --fixed-fps 60 --disable-vsync --quit-after 900 \
  --write-movie "$run_dir/control.avi" -- --control-demo >"$run_dir/godot.log" 2>&1
rg -q '^CONTROL_OK ticks=300 hits=1 damage=18 replayed=120 rows_and_mesh=exact' "$run_dir/godot.log"
rg -q '^CONTROL_CAPTURE_OK' "$run_dir/godot.log"
lab_encode "$run_dir/control.avi" "$run_dir/control.mp4"
lab_probe "$run_dir/control.mp4" >"$run_dir/media.json"
jq -e '.ticks == 300 and .hits == 1 and .damage == 18 and .replayed == 120 and .hit_ticks == [91]' "$run_dir/control-proof.json"
jq -e '.streams[0] | .codec_name == "h264" and .width == 960 and .height == 540 and (.nb_frames|tonumber)>=660' "$run_dir/media.json"
printf 'CONTROL_CAPTURE_OK artifacts=%s\n' "$run_dir"
