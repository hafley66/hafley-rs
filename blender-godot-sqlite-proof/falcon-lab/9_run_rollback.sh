#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
cargo test --locked
cargo run --locked --bin falcon-rollback
ffmpeg -hide_banner -loglevel error -y -i 10_peer0.mp4 -i 10_peer1.mp4 -filter_complex '[0:v][1:v]hstack=inputs=2[v]' -map '[v]' -an -c:v libx264 -crf 18 -pix_fmt yuv420p -movflags +faststart 12_rollback.mp4
ffmpeg -hide_banner -loglevel error -y -i 12_rollback.mp4 -vf 'select=eq(n\,230)+eq(n\,315)+eq(n\,380)+eq(n\,600),scale=1280:-2,tile=1x4' -frames:v 1 13_rollback_frames.png
ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,nb_frames:format=duration,size -of json 12_rollback.mp4
