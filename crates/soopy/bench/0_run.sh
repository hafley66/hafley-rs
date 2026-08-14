#!/usr/bin/env bash
set -euo pipefail

for command in cargo git jq; do
    command -v "$command" >/dev/null || {
        printf 'missing required command: %s\n' "$command" >&2
        exit 127
    }
done

repo=${1:?repository path required}
handles=${2:?handle count required}
batch=${3:?read batch size required}
label=${4:?receipt label required}
shift 4

repo=$(git -C "$repo" rev-parse --show-toplevel)
revision=$(git -C "$repo" rev-parse HEAD)
receipt_dir=target/soopy-scale
receipt="$receipt_dir/$label.json"
resources="$receipt_dir/$label.resources.txt"
mkdir -p "$receipt_dir"

args=(--repo "$repo" --revision "$revision" --handles "$handles" --batch "$batch")
for pathspec in "$@"; do
    if [[ -n $pathspec ]]; then
        args+=(--pathspec "$pathspec")
    fi
done

if [[ $(uname -s) == Darwin ]]; then
    /usr/bin/time -l cargo run --quiet --release -p soopy --example 0_scale -- "${args[@]}" >"$receipt" 2>"$resources"
else
    /usr/bin/time -v cargo run --quiet --release -p soopy --example 0_scale -- "${args[@]}" >"$receipt" 2>"$resources"
fi

jq . "$receipt"
printf 'resource receipt: %s\n' "$resources"
