#!/bin/bash
set -euo pipefail
rm -f .dogfood/D02.db
"$RYII" fast packages > .dogfood/D02.fast.jsonl
"$RYII" --resolve --sqlite .dogfood/D02.db --pattern '**/*.ts' --pattern '**/*.tsx' packages > .dogfood/D02.resolve.jsonl
sqlite3 -json .dogfood/D02.db "select distinct _input_path as path, coalesce(module,name) as module from specifier where coalesce(module,name) like '@hafley66/%' order by 1,2" > .dogfood/D02.pairs.json
sqlite3 -json .dogfood/D02.db "select distinct src_path, name from resolved_import where kind='module' and name like '@hafley66/%' order by 1,2" > .dogfood/D02.sqlite.json
node <<'JS'
const fs = require('fs');
const rows = fs.readFileSync('.dogfood/D02.fast.jsonl', 'utf8').trim().split('\n').map(JSON.parse);
const pairs = JSON.parse(fs.readFileSync('.dogfood/D02.pairs.json', 'utf8'));
const fast = new Set(rows.filter(row => row.record === 'resolved_import' && row.kind === 'module')
  .map(row => JSON.stringify([row.src_path, row.name])));
const sqlite = new Set(JSON.parse(fs.readFileSync('.dogfood/D02.sqlite.json', 'utf8'))
  .map(row => JSON.stringify([row.src_path, row.name])));
let missing = 0;
for (const {path, module} of pairs) {
  const key = JSON.stringify([path, module]);
  if (!fast.has(key) || !sqlite.has(key)) {
    missing++;
    console.error(`unresolved ${key}: fast=${fast.has(key)} sqlite=${sqlite.has(key)}; export target did not reach a corpus source file`);
  }
}
if (!pairs.length || missing) throw new Error(`D2: ${missing}/${pairs.length} unresolved pairs`);
console.log(`D2: ${pairs.length}/${pairs.length} pairs resolved in fast and SQLite; source fallback rungs covered by workspace_exports_resolve_to_sources_in_rung_order`);
JS
