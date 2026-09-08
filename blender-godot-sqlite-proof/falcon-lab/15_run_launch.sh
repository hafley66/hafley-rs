#!/usr/bin/env bash
set -euo pipefail
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
cd "$lab_dir"
lab_cargo test -- --test-threads=1
lab_cargo run --bin falcon-rollback -- --launch
ffmpeg -hide_banner -loglevel error -y -i 16_peer0.mp4 -i 16_peer1.mp4 -filter_complex '[0:v][1:v]hstack=inputs=2[v]' -map '[v]' -an -c:v libx264 -crf 18 -pix_fmt yuv420p -movflags +faststart 18_launch.mp4
ffmpeg -hide_banner -loglevel error -y -i 18_launch.mp4 -vf 'select=eq(n\,315)+eq(n\,450)+eq(n\,540)+eq(n\,620)+eq(n\,790),scale=1280:-2,tile=1x5' -frames:v 1 19_launch_frames.png
lab_probe 18_launch.mp4
