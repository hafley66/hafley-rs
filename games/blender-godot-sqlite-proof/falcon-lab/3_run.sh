#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
cargo test --locked
cargo run --locked
ffmpeg -hide_banner -loglevel error -y -i 5_falcon_knee.mp4 -vf 'select=eq(n\,30)+eq(n\,68)+eq(n\,85)+eq(n\,91)+eq(n\,100)+eq(n\,145),scale=640:-2,tile=3x2' -frames:v 1 6_frames.png
ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,nb_frames:format=duration,size -of json 5_falcon_knee.mp4
