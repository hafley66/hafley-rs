#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
blender --background --python 0_make_model.py
godot --headless --path godot --editor --import
bash 4_record.sh
bash 10_record_collision.sh
bash 14_record_gpu.sh
