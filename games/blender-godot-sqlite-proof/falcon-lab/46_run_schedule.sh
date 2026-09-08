#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
unset CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER
cargo test -j 2 --locked --offline -- --test-threads=1
cargo build -j 2 --locked --offline --features gdext --lib
godot --headless --path godot --script res://1_load.gd
godot --headless --path godot --check-only --script res://2_stage.gd
godot --path godot --fixed-fps 60 --disable-vsync --quit-after 1800 --write-movie "$PWD/51_schedule.avi" -- --scheduled | tee 53_schedule_run.log
rg -q '^SCHEDULE_OK ticks=180 full_states=360 exact skipped_publications=12 consumer_latest=verified cursor_isolation=verified' 53_schedule_run.log
rg -q '^SCHEDULE_CAPTURE_OK' 53_schedule_run.log
ffmpeg -hide_banner -loglevel error -y -i 51_schedule.avi -an -c:v libx264 -threads 2 -preset veryfast -crf 20 -pix_fmt yuv420p -movflags +faststart 49_schedule.mp4
ffmpeg -hide_banner -loglevel error -y -i 49_schedule.mp4 -vf 'fps=2,scale=480:-1,tile=4x4' -frames:v 1 50_schedule_frames.png
ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,nb_frames:format=duration,size -of json 49_schedule.mp4
