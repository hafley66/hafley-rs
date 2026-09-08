#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
unset CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER
cargo test -j 2 --locked --offline -- --test-threads=1
cargo build -j 2 --locked --offline --features gdext --lib
godot --headless --path godot --script res://1_load.gd
godot --headless --path godot --check-only --script res://2_stage.gd
python3 54_fault_controller.py
ffmpeg -hide_banner -loglevel error -y -i 62_faults.avi -an -c:v libx264 -threads 2 -preset veryfast -crf 20 -pix_fmt yuv420p -movflags +faststart 60_faults.mp4
ffmpeg -hide_banner -loglevel error -y -i 60_faults.mp4 -vf 'fps=2,scale=480:-1,tile=4x3' -frames:v 1 61_fault_frames.png
ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,nb_frames:format=duration,size -of json 60_faults.mp4
