#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
unset CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER
cargo test -j 2 --locked --offline -- --test-threads=1
cargo build -j 2 --locked --offline --features gdext --lib
godot --headless --path godot --script res://1_load.gd
godot --path godot --fixed-fps 60 --disable-vsync --write-movie "$PWD/44_incremental.avi" -- --incremental
ffmpeg -hide_banner -loglevel error -y -i 44_incremental.avi -an -frames:v 824 -c:v libx264 -threads 2 -preset veryfast -crf 20 -pix_fmt yuv420p -movflags +faststart 41_incremental.mp4
ffmpeg -hide_banner -loglevel error -y -i 41_incremental.mp4 -vf 'select=eq(n\,315)+eq(n\,390)+eq(n\,450)+eq(n\,620),tile=1x4' -frames:v 1 42_incremental_frames.png
ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,nb_frames:format=duration,size -of json 41_incremental.mp4
test "$(ffprobe -v error -select_streams v:0 -show_entries stream=nb_frames -of csv=p=0 41_incremental.mp4)" = 824
