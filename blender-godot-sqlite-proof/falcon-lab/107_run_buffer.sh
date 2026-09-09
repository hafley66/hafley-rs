#!/usr/bin/env bash
set -euo pipefail
lab_dir=$(cd "$(dirname "$0")" && pwd)
. "$lab_dir/0_shell.sh"
case "${1:-buffer}" in
  buffer|buffer-held) capture_dir="${1:-buffer}" ;;
  *) exit 2 ;;
esac
mkdir -p "$lab_dir/.workflow/$capture_dir"
cd "$lab_dir/.workflow/$capture_dir"
lab_cargo run --bin falcon-rollback -- --buffer-proof
lab_probe buffer-proof.mp4 > probe.json
ffmpeg -hide_banner -loglevel error -y -ss 1.4 -i buffer-proof.mp4 -frames:v 1 comparison.png
ffmpeg -hide_banner -loglevel error -y -ss 5.1 -i buffer-proof.mp4 -frames:v 1 cancellation.png
lab_probe held-proof.mp4 > held-probe.json
ffmpeg -hide_banner -loglevel error -y -ss 3.35 -i held-proof.mp4 -frames:v 1 held-comparison.png
