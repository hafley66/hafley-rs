#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
godot --headless --path godot --script res://2_verify.gd
godot --path godot --script res://3_render.gd --resolution 800x600 --fixed-fps 30 --disable-vsync --write-movie "$PWD/5_import-proof.avi" --quit-after 180
ffmpeg -hide_banner -loglevel error -y -i 5_import-proof.avi -an -vf scale=800:-2:out_range=tv -color_range tv -c:v libx264 -preset veryfast -crf 23 -pix_fmt yuv420p -movflags +faststart 6_import-proof.mp4
ffmpeg -hide_banner -loglevel error -y -ss 0.5 -i 6_import-proof.mp4 -frames:v 1 7_preview.png
ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,pix_fmt,nb_frames:format=duration,size -of json 6_import-proof.mp4
