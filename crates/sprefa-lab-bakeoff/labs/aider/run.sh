#!/usr/bin/env bash
# usage: run.sh <case-id>   writes out/aider/<case-id>.json (one CaseAnswer),
# out/aider/maps/<case-id>.map.txt (mechanism 1) and
# out/aider/tags/<case-id>.tsv (mechanism 2).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
case_id="${1:?usage: run.sh <case-id>}"
mkdir -p "$here/../../out/aider/maps"
cd "$here"
files="$(.venv/bin/python -c 'import sys; from cases import FILES; print(" ".join(FILES[sys.argv[1]]))' "$case_id")"
# shellcheck disable=SC2086
./map.sh "$here/../../out/aider/maps/$case_id.map.txt" $files
.venv/bin/python cases.py "$case_id"
