#!/usr/bin/env bash
set -euo pipefail
lab_dir=$(cd "$(dirname "$0")" && pwd)
. "$lab_dir/0_shell.sh"
mkdir -p "$lab_dir/.workflow/buffer"
cd "$lab_dir/.workflow/buffer"
lab_cargo run --bin falcon-rollback -- --buffer-proof
lab_probe buffer-proof.mp4 > probe.json
ffmpeg -hide_banner -loglevel error -y -ss 1.4 -i buffer-proof.mp4 -frames:v 1 comparison.png
ffmpeg -hide_banner -loglevel error -y -ss 5.1 -i buffer-proof.mp4 -frames:v 1 cancellation.png
