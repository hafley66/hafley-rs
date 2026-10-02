#!/bin/bash
# Requires the fast JSX record contract from feature-ryi-ts-rtkq-jsx.
set -euo pipefail
: "${RYII:?}" "${STATE:?}" "${CORPUS:?}"
fixture="$STATE/J01.fixture"
mkdir -p "$fixture"
ln -s "$CORPUS/node_modules" "$fixture/node_modules"
cat >"$fixture/tsconfig.json" <<'EOF'
{"compilerOptions":{"strict":true,"target":"ES2022","module":"ESNext","moduleResolution":"Bundler","jsx":"preserve","noEmit":true},"include":["*.ts","*.tsx"]}
EOF
cat >"$fixture/0_jsx.d.ts" <<'EOF'
declare namespace JSX {
  interface Element {}
  interface IntrinsicElements { div: { "data-bar"?: number } }
}
EOF
cat >"$fixture/1_components.tsx" <<'EOF'
export interface FooProps { bar: number }
export function Foo({ bar }: FooProps): JSX.Element { return <div data-bar={bar}/> }
export interface OtherProps { bar: string }
export function Other({ bar }: OtherProps): JSX.Element { return <div/> }
EOF
cat >"$fixture/2_view.tsx" <<'EOF'
import { Foo } from "./1_components.js";
const marker = "😀";
const x = 42;
export const view = <Foo bar={x}/>;
EOF
RUST_LOG=off "$RYII" --resolve --ts-checker --root "$fixture" --sqlite "$STATE/J01.resolve.db" "$fixture" >/dev/null
RUST_LOG=off "$RYII" slow --root "$fixture" --sqlite "$STATE/J01.slow.db" "$fixture" >/dev/null
for tier in resolve slow; do
  for table in jsx_element jsx_attribute resolved_edge occurrence; do
    sqlite3 -json "$STATE/J01.$tier.db" "SELECT * FROM $table;" >"$STATE/J01.$table.json"
  done
  node - "$fixture" "$STATE" <<'JS'
const fs = require('node:fs');
const assert = require('node:assert/strict');
const [fixture, state] = process.argv.slice(2);
const rows = name => JSON.parse(fs.readFileSync(`${state}/J01.${name}.json`, 'utf8') || '[]');
const view = fs.readFileSync(`${fixture}/2_view.tsx`, 'utf8');
const components = fs.readFileSync(`${fixture}/1_components.tsx`, 'utf8');
const byte = (text, at) => Buffer.byteLength(text.slice(0, at));
const elementStart = byte(view, view.indexOf('<Foo'));
const attrStart = byte(view, view.indexOf('bar={x}'));
const propStart = byte(components, components.indexOf('bar: number'));
const elements = rows('jsx_element').filter(row => row.path.endsWith('/2_view.tsx'));
assert.deepEqual(elements.map(row => [row.name, row.start]), [['Foo', elementStart]]);
const attrs = rows('jsx_attribute').filter(row => row.path.endsWith('/2_view.tsx'));
assert.deepEqual(attrs.map(row => [row.name, row.element_start, row.start, row.value]),
  [['bar', elementStart, attrStart, '{x}']]);
const edges = rows('resolved_edge').filter(row => row.caller_path.endsWith('/2_view.tsx')
  && row.caller_site_start === elementStart && row.callee_name === 'Foo');
assert.ok(edges.length > 0);
assert.ok(edges.every(row => row.callee_path.endsWith('/1_components.tsx')
  && row.resolution_origin === 'checker'));
const occurrences = rows('occurrence');
const refs = occurrences.filter(row => row.path.endsWith('/2_view.tsx') && row.role === 'ref'
  && row.start === attrStart && row.end === attrStart + 3 && row.symbol.startsWith('tsgo '));
assert.equal(refs.length, 1);
assert.ok(refs[0].symbol.includes('#FooProps.bar@'));
const defs = occurrences.filter(row => row.symbol === refs[0].symbol && row.role === 'def');
assert.equal(defs.length, 1);
assert.ok(defs[0].path.endsWith('/1_components.tsx')
  && defs[0].start <= propStart && propStart < defs[0].end);
JS
done
echo 'J01 resolve + slow facts: <Foo bar={x}/> binds Foo and FooProps.bar; UTF-8 spans asserted'
