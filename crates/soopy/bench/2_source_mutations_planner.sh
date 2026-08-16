#!/usr/bin/env bash
set -euo pipefail

for command in cargo jq; do
    command -v "$command" >/dev/null || {
        printf 'missing required command: %s\n' "$command" >&2
        exit 127
    }
done

files=${1:-1000}
edits_per_file=${2:-100}
bytes_per_file=${3:-4096}
receipt_dir=target/perf-source-mutations-planner
receipt="$receipt_dir/receipt.json"
resources="$receipt_dir/resources.txt"
mkdir -p "$receipt_dir"

if [[ $(uname -s) == Darwin ]]; then
    /usr/bin/time -l cargo run --quiet --release -p soopy --example 2_mutation_plan_scale -- \
        --files "$files" --edits-per-file "$edits_per_file" --bytes-per-file "$bytes_per_file" \
        >"$receipt" 2>"$resources"
    peak_rss=$(awk '/maximum resident set size/ { print $1 }' "$resources")
else
    /usr/bin/time -v cargo run --quiet --release -p soopy --example 2_mutation_plan_scale -- \
        --files "$files" --edits-per-file "$edits_per_file" --bytes-per-file "$bytes_per_file" \
        >"$receipt" 2>"$resources"
    peak_rss_kib=$(awk -F ': *' '/Maximum resident set size/ { print $2 }' "$resources")
    if [[ -n ${peak_rss_kib:-} ]]; then
        peak_rss=$((peak_rss_kib * 1024))
    else
        peak_rss=
    fi
fi

jq --argjson peak_rss_bytes "${peak_rss:-null}" '. + {peak_rss_bytes: $peak_rss_bytes}' "$receipt" >"$receipt.tmp"
mv -f "$receipt.tmp" "$receipt"
jq . "$receipt"
