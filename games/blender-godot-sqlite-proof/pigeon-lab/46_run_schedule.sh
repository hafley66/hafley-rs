#!/usr/bin/env bash
set -euo pipefail
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
cd "$lab_dir"
lab_cargo test -- --test-threads=1
lab_cargo build --features gdext --lib
godot --headless --path godot --script res://1_load.gd
godot --headless --path godot --check-only --script res://2_stage.gd
godot --path godot --fixed-fps 60 --disable-vsync --quit-after 1800 --write-movie "$PWD/51_schedule.avi" -- --scheduled | tee 53_schedule_run.log
rg -q '^SCHEDULE_OK ticks=180 full_states=360 exact skipped_publications=12 consumer_latest=verified cursor_isolation=verified' 53_schedule_run.log
rg -q '^SCHEDULE_CAPTURE_OK' 53_schedule_run.log
lab_encode 51_schedule.avi 49_schedule.mp4
ffmpeg -hide_banner -loglevel error -y -i 49_schedule.mp4 -vf 'fps=2,scale=480:-1,tile=4x4' -frames:v 1 50_schedule_frames.png
lab_probe 49_schedule.mp4
