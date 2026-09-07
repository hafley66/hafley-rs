#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
cargo test --locked
cargo run --locked --bin falcon-rollback -- --launch
ffmpeg -hide_banner -loglevel error -y -i 16_peer0.mp4 -i 16_peer1.mp4 -filter_complex '[0:v][1:v]hstack=inputs=2[v]' -map '[v]' -an -c:v libx264 -crf 18 -pix_fmt yuv420p -movflags +faststart 18_launch.mp4
ffmpeg -hide_banner -loglevel error -y -i 18_launch.mp4 -vf 'select=eq(n\,315)+eq(n\,450)+eq(n\,540)+eq(n\,620)+eq(n\,790),scale=1280:-2,tile=1x5' -frames:v 1 19_launch_frames.png
ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,nb_frames:format=duration,size -of json 18_launch.mp4
