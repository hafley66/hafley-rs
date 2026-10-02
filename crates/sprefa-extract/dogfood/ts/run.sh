#!/bin/bash
# TS dogfood gate: every D*.sh / J*.sh beside this file, one corpus reset before each.
# Env: RYII (ryii binary built with --features cli,ts-checker), CORPUS (dir; created via 0_corpus.sh),
# STATE (soopy state root outside CORPUS). Each case script exits 0 on the expected result and prints one line.
# Usage: run.sh [case-name...]
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
: "${RYII:?RYII=path to ryii}" "${CORPUS:?CORPUS=corpus dir}" "${STATE:=${CORPUS}.state}"
export RYII CORPUS STATE
cases=("$@"); [ ${#cases[@]} -gt 0 ] || cases=($(cd "$here" && ls [DJ][0-9]*.sh 2>/dev/null | sed 's/\.sh$//'))
pass=0; fail=0
for c in "${cases[@]}"; do
  "$here/0_corpus.sh" "$CORPUS" >/dev/null; rm -rf "$STATE"; mkdir -p "$STATE"
  start=$(date +%s)
  if out=$(cd "$CORPUS" && bash "$here/$c.sh" 2>&1); then pass=$((pass+1)); s=ok; else fail=$((fail+1)); s="not ok"; fi
  printf '%s %s (%ss) %s\n' "$s" "$c" "$(( $(date +%s) - start ))" "$(tail -1 <<<"$out")"
done
echo "ts dogfood: $pass ok, $fail not ok"
[ "$fail" -eq 0 ]
