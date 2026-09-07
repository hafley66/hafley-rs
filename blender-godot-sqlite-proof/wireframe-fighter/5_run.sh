#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
blender --background --python 0_build_fighter.py
godot --headless --path . --editor --import
godot --path . res://4_main.tscn --fixed-fps 30 --disable-vsync --write-movie "$PWD/6_wire_fighter.avi" --quit-after 180
ffmpeg -hide_banner -loglevel error -y -i 6_wire_fighter.avi -an -vf scale=800:-2:out_range=tv -color_range tv -c:v libx264 -preset veryfast -crf 20 -pix_fmt yuv420p -movflags +faststart 7_wire_fighter.mp4
ffmpeg -hide_banner -loglevel error -y -i 7_wire_fighter.mp4 -vf 'select=eq(n\,0)+eq(n\,15)+eq(n\,30)+eq(n\,45)+eq(n\,60)+eq(n\,75),scale=400:-2,tile=3x2' -frames:v 1 8_frames.png
ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,nb_frames:format=duration,size -of json 7_wire_fighter.mp4
