#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
trace_path="$PWD/core-labs/results/0_trace.json"
cargo test --manifest-path core-labs/Cargo.toml
cargo run --manifest-path core-labs/Cargo.toml -- "$trace_path"
trace_fps=$(jq -er '.fps | select(. > 0)' "$trace_path")
godot --path godot --script res://9_render_collision.gd --fixed-fps "$trace_fps" --disable-vsync --write-movie "$PWD/11_collision.avi" -- "$trace_path"
ffmpeg -hide_banner -loglevel error -y -i 11_collision.avi -an -vf scale=800:-2:out_range=tv -color_range tv -c:v libx264 -preset veryfast -crf 23 -pix_fmt yuv420p -movflags +faststart 12_collision.mp4
ffmpeg -hide_banner -loglevel error -y -i 12_collision.mp4 -vf 'select=eq(n\,1)+eq(n\,35)+eq(n\,70)+eq(n\,105)+eq(n\,140)+eq(n\,175),scale=400:-2,tile=3x2' -frames:v 1 13_collision_frames.png
ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,nb_frames:format=duration,size -of json 12_collision.mp4
