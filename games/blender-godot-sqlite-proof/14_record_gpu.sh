#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
trace_path="$PWD/core-labs/results/0_trace.json"
test -f "$trace_path"
shasum -a 256 "$trace_path"
cargo run --locked --manifest-path render-lab/Cargo.toml -- "$trace_path" "$PWD/15_gpu_collision.mp4"
ffmpeg -hide_banner -loglevel error -y -i 15_gpu_collision.mp4 -vf 'select=eq(n\,1)+eq(n\,35)+eq(n\,70)+eq(n\,105)+eq(n\,140)+eq(n\,175),scale=400:-2,tile=3x2' -frames:v 1 16_gpu_frames.png
ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,nb_frames:format=duration,size -of json 15_gpu_collision.mp4
