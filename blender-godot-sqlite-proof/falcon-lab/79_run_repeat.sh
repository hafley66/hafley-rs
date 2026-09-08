#!/bin/sh
set -eu
lab_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$lab_dir/0_shell.sh"
lab_cargo build --features gdext --lib --bin falcon-rollback
run_dir=$(lab_temp repeat)
cd "$run_dir"
"$lab_dir/target/debug/falcon-rollback" --repeat
lab_probe repeat.mp4 > media.json
jq -e '.streams[0] | .codec_name == "h264" and .width == 960 and .height == 540 and (.nb_frames|tonumber)>0' media.json
jq -e '.ticks == 300 and .hits == 2 and .damage == 36 and .replayed == 120 and .hit_ticks == [91,211]' repeat-proof.json
printf 'REPEAT_CAPTURE_OK artifacts=%s\n' "$run_dir"
