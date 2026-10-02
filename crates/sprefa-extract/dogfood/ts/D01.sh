#!/bin/bash
set -euo pipefail
# run.sh supplies a reset, lane-owned corpus and an external state directory.
for tier in fast slow; do
  flags=(); [ "$tier" != slow ] || flags=(--slow)
  for symbol in toSignal GraphId; do
    git reset -q --hard 2145cb14
    rm -rf "$STATE"; mkdir -p "$STATE"
    if [ "$symbol" = toSignal ]; then
      anchor=packages/signals/src/2_Signal.ts
      replacement=asSignal
      expected=22
      packages=(signals boop-xterm)
    else
      anchor=packages/grapht-model/src/6_graph.ts
      replacement=GraphKey
      expected=31
      packages=(grapht-model grapht grapht-golden scene)
    fi
    for pkg in "${packages[@]}"; do
      ./node_modules/.bin/tsc --noEmit -p ".dogfood/tsconfig.$pkg.json" > ".dogfood/D01.before.$pkg.txt" 2>&1 || true
    done
    "$RYII" rename "$anchor#$symbol" "$replacement" --state "$STATE" --json --commit "${flags[@]}" > ".dogfood/D01.$tier.$symbol.txt"
    count=$(git diff --name-only | wc -l | tr -d ' ')
    [ "$count" -ge "$expected" ] || { echo "D1: $tier $symbol files=$count expected >=$expected"; exit 1; }
    # Assert successful commit has no abstains, then compare tsc diagnostics.
    tail -1 ".dogfood/D01.$tier.$symbol.txt" | node -e 'let s="";process.stdin.on("data",x=>s+=x);process.stdin.on("end",()=>{if(JSON.parse(s).abstains.length)process.exit(1)})'
    for pkg in "${packages[@]}"; do
      ./node_modules/.bin/tsc --noEmit -p ".dogfood/tsconfig.$pkg.json" > ".dogfood/D01.after.$pkg.txt" 2>&1 || true
      PKG="$pkg" node <<'JS'
const fs = require('fs');
const pkg = process.env.PKG;
const errors = side => fs.readFileSync(`.dogfood/D01.${side}.${pkg}.txt`, 'utf8')
  .split('\n').filter(line => /error TS\d+:/.test(line));
const before = new Set(errors('before'));
const added = errors('after').filter(line => !before.has(line));
if (added.length) { console.error(added.join('\n')); process.exit(1); }
JS
    done
    echo "D1: $tier $symbol files=$count; zero new tsc diagnostics"
  done
done
git reset -q --hard 2145cb14
echo 'D1: fast and slow cover toSignal >=22, GraphId >=31; zero new tsc diagnostics'
