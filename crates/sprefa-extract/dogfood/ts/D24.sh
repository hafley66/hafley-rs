#!/bin/bash
set -euo pipefail
"$RYII" fast packages/md > .dogfood/D24.fast.jsonl
rm -f .dogfood/D24.db
"$RYII" fast --sqlite .dogfood/D24.db packages/md > .dogfood/D24.sqlite.txt
node <<'JS'
const fs = require('fs');
const rows = fs.readFileSync('.dogfood/D24.fast.jsonl', 'utf8').trim().split('\n').map(JSON.parse);
const files = new Set(rows.filter(row => row.record === 'file').map(row => row.path));
for (const path of ['packages/md/src/0_css.d.ts', 'packages/md/src/style.css.d.ts']) {
  if (!files.has(path)) throw new Error(`D24: missing fast file row for ${path}`);
}
JS
count=$(sqlite3 .dogfood/D24.db "select count(distinct path) from file where path in ('packages/md/src/0_css.d.ts','packages/md/src/style.css.d.ts')")
[ "$count" -eq 2 ]
echo 'D24: fast JSONL and SQLite include 0_css.d.ts and style.css.d.ts'
