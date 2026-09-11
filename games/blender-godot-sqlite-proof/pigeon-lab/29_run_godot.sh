#!/usr/bin/env bash
set -euo pipefail
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
cd "$lab_dir"
lab_cargo test -- --test-threads=1
lab_cargo_manifest "$lab_dir/../core-labs/Cargo.toml" test -- --test-threads=1
lab_cargo build --features gdext --lib
godot --headless --path godot --script res://1_load.gd
godot --path godot --fixed-fps 60 --disable-vsync --write-movie "$PWD/34_godot_sql.avi"
# Movie Maker appends a duplicate final frame on shutdown in this runtime.
lab_encode 34_godot_sql.avi 31_godot_sql.mp4 -frames:v 824
ffmpeg -hide_banner -loglevel error -y -i 31_godot_sql.mp4 -vf 'select=eq(n\,315)+eq(n\,390)+eq(n\,450)+eq(n\,620),tile=1x4' -frames:v 1 32_godot_frames.png
lab_probe 31_godot_sql.mp4
test "$(ffprobe -v error -select_streams v:0 -show_entries stream=nb_frames -of csv=p=0 31_godot_sql.mp4)" = 824
