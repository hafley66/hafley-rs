#!/usr/bin/env bash
set -euo pipefail
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
cd "$lab_dir"
lab_cargo test -- --test-threads=1
lab_cargo_manifest "$lab_dir/../core-labs/Cargo.toml" test -- --test-threads=1
lab_cargo run --bin falcon-rollback -- --sql
ffmpeg -hide_banner -loglevel error -y -i 23_sql_boundary.mp4 -vf 'select=eq(n\,315)+eq(n\,390)+eq(n\,450)+eq(n\,620),tile=1x4' -frames:v 1 25_sql_frames.png
lab_probe 23_sql_boundary.mp4
