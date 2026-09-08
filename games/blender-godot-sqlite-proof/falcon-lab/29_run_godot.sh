#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
# Do not inherit the launching application's custom linker/signing script.
unset CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER
cargo test -j 2 --locked --offline -- --test-threads=1
cargo test -j 2 --locked --offline --manifest-path ../core-labs/Cargo.toml -- --test-threads=1
cargo build -j 2 --locked --offline --features gdext --lib
godot --headless --path godot --script res://1_load.gd
godot --path godot --fixed-fps 60 --disable-vsync --write-movie "$PWD/34_godot_sql.avi"
# Movie Maker appends a duplicate final frame on shutdown in this runtime.
ffmpeg -hide_banner -loglevel error -y -i 34_godot_sql.avi -an -frames:v 824 -c:v libx264 -threads 2 -preset veryfast -crf 20 -pix_fmt yuv420p -movflags +faststart 31_godot_sql.mp4
ffmpeg -hide_banner -loglevel error -y -i 31_godot_sql.mp4 -vf 'select=eq(n\,315)+eq(n\,390)+eq(n\,450)+eq(n\,620),tile=1x4' -frames:v 1 32_godot_frames.png
ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,nb_frames:format=duration,size -of json 31_godot_sql.mp4
test "$(ffprobe -v error -select_streams v:0 -show_entries stream=nb_frames -of csv=p=0 31_godot_sql.mp4)" = 824
