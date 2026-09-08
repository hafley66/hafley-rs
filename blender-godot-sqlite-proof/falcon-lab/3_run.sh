#!/usr/bin/env bash
set -euo pipefail
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
cd "$lab_dir"
lab_cargo test -- --test-threads=1
lab_cargo run
ffmpeg -hide_banner -loglevel error -y -i 5_falcon_knee.mp4 -vf 'select=eq(n\,30)+eq(n\,68)+eq(n\,85)+eq(n\,91)+eq(n\,100)+eq(n\,145),scale=640:-2,tile=3x2' -frames:v 1 6_frames.png
lab_probe 5_falcon_knee.mp4
