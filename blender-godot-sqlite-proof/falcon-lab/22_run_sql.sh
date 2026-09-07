#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
cargo test -j 2 --locked --offline -- --test-threads=1
cargo test -j 2 --locked --offline --manifest-path ../core-labs/Cargo.toml -- --test-threads=1
cargo run -j 2 --locked --offline --bin falcon-rollback -- --sql
ffmpeg -hide_banner -loglevel error -y -i 23_sql_boundary.mp4 -vf 'select=eq(n\,315)+eq(n\,390)+eq(n\,450)+eq(n\,620),tile=1x4' -frames:v 1 25_sql_frames.png
ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,nb_frames:format=duration,size -of json 23_sql_boundary.mp4
