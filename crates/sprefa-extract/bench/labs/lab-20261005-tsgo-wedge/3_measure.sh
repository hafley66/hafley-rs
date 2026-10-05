#!/usr/bin/env bash
# One measured run per (wedge, question); appends rows to results.tsv. Needs 0_clone.sh's two binaries and a release ryii.
set -euo pipefail
lab="$(cd "$(dirname "$0")" && pwd)"
bench="$(cd "$lab/../.." && pwd)"
corpus="${CORPUS:-/Users/chrishafley/projects/hafley-rxjs}"
ryii="${RYII:-$bench/../target/release/ryii}"
base="$bench/repos/tsgo-bin/tsgo-base"
patched="$bench/repos/tsgo-bin/tsgo-wedge"
client="$lab/2_wedges/client.ts"
scratch="$bench/repos/lab-20261005-tsgo-wedge"
mkdir -p "$scratch"; rm -f "$scratch"/truth.*
results="$lab/results.tsv"
[ -s "$results" ] || printf 'wedge\tquestion\tbinary\twall_s\tuser_s\tsys_s\tmax_rss_mb\trequests\tfiles_opened\tanswers\tanswers_equal\n' > "$results"

metric() { awk -v key="$2" '$0 ~ key { print $1; exit }' "$1"; }
seconds() { awk -v key="$2" '$0 ~ "real" { for (i = 1; i < NF; i++) if ($(i + 1) == key) { print $i; exit } }' "$1"; }

tag() { printf '%s' "$1" | tr -c 'A-Za-z0-9_.-' '_'; }

# Today's ryi slow tier (LSP didOpen + one definition per site), whole command. Its Route edges are the Route truth.
ryii_row() {
  local name="$1" declared="$2" question="$3" key; key="$(tag "$question")"
  (cd "$corpus" && /usr/bin/time -l "$ryii" graph --slow --timeout 110 --callers "$declared#$name" packages \
    > "$scratch/ryii.$key.out" 2> "$scratch/ryii.$key.time")
  jq -r '"\(.from_path):\(.from_line)"' "$scratch/ryii.$key.out" | sort -u > "$scratch/ryii.$key.edges"
  local equal=truth
  if [ -s "$scratch/truth.$key" ]; then cmp -s "$scratch/ryii.$key.edges" "$scratch/truth.$key" && equal=yes || equal=no
  else cp "$scratch/ryii.$key.edges" "$scratch/truth.$key"; fi
  printf 'ryii-today\t%s\tnpm typescript@7.0.2\t%s\t%s\t%s\t%s\t\t\t%s\t%s\n' "$question" \
    "$(seconds "$scratch/ryii.$key.time" real)" "$(seconds "$scratch/ryii.$key.time" user)" "$(seconds "$scratch/ryii.$key.time" sys)" \
    "$(( $(metric "$scratch/ryii.$key.time" "maximum resident") / 1048576 ))" "$(wc -l < "$scratch/ryii.$key.edges" | tr -d ' ')" "$equal" >> "$results"
}

# The first run of a question without truth sets it: ryii for Route, today's protocol (lsp-open) for Signal,
# the stock API's isTypeAssignableTo (same checker function, one request per pair) for relations.
run() {
  local wedge="$1" question="$2" binary="$3" key; key="$(tag "$question")"
  local json="$scratch/$wedge.$key.json" answers equal
  /usr/bin/time -l node "$client" "$wedge" "$question" "$corpus" "$binary" > "$json" 2> "$scratch/$wedge.$key.time"
  if jq -e '.answers | type == "object"' "$json" > /dev/null; then
    answers="$(jq '(.answers.edges // .answers.relations) | length' "$json")"
    jq -r '(.answers.edges // .answers.relations)[]' "$json" | sort -u > "$scratch/$wedge.$key.answers"
    if [ -s "$scratch/truth.$key" ]; then cmp -s "$scratch/$wedge.$key.answers" "$scratch/truth.$key" && equal=yes || equal=no
    else cp "$scratch/$wedge.$key.answers" "$scratch/truth.$key"; equal=truth; fi
  else
    answers=0; equal=unreachable
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$wedge" "$question" "$(basename "$binary")" \
    "$(seconds "$scratch/$wedge.$key.time" real)" "$(seconds "$scratch/$wedge.$key.time" user)" \
    "$(seconds "$scratch/$wedge.$key.time" sys)" "$(( $(metric "$scratch/$wedge.$key.time" "maximum resident") / 1048576 ))" \
    "$(jq .requests "$json")" "$(jq .files_opened "$json")" "$answers" "$equal" >> "$results"
}

ryii_row Route packages/signals/src/5_Route.ts callers
signal="callers=Signal@packages/signals/src/2_Signal.ts"
for question in callers "$signal"; do
  run lsp-open "$question" "$base"
  run lsp-noopen "$question" "$base"
  run api "$question" "$base"
  run api-batch "$question" "$patched"
  run lsp-batch "$question" "$patched"
done
ryii_row Signal packages/signals/src/2_Signal.ts "$signal"
run api relations "$base"
run api-batch relations "$patched"
run lsp-batch relations "$patched"
run lsp-open relations "$base"
column -t -s $'\t' "$results"
