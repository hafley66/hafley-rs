#!/usr/bin/env bash
set -euo pipefail
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
cd "$lab_dir"
lab_cargo test -- --test-threads=1
lab_cargo build --features gdext --lib
godot --headless --path godot --script res://1_load.gd
godot --headless --path godot --check-only --script res://2_stage.gd
python3 54_fault_controller.py
lab_encode 62_faults.avi 60_faults.mp4
ffmpeg -hide_banner -loglevel error -y -i 60_faults.mp4 -vf 'fps=2,scale=480:-1,tile=4x3' -frames:v 1 61_fault_frames.png
lab_probe 60_faults.mp4
