// Read-only provenance check. Does not interpret, execute or redistribute the codesets.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';

const root = process.argv[2];
assert.ok(root, 'Usage: node tools/0_pm_codes_receipt.mjs <PM-CC checkout>');
function read(name, encoding, sha256) {
  const bytes = readFileSync(resolve(root, '[Dev Resources]', name));
  assert.equal(createHash('sha256').update(bytes).digest('hex'), sha256, name);
  return bytes.toString(encoding).split(/\r?\n/);
}
const raw = read('codes-3_6.txt', 'utf8',
  '0a2f08b47e06306dd675dc901c0e35c88fddd1b82fd95ad5b43d68004902142a');
const annotated = read('codes-3_61.txt', 'utf16le',
  '1065affec29049d4096ab802f5cd69f4e38cc24c6a0b4ea21f6920ffab9126ba');
const rows = raw.map((text, index) => ({ text, line: index + 1 }))
  .filter(row => /^[A-F0-9]{8} [A-F0-9]{8}$/.test(row.text));
const receipts = [];
for (const [prefix, expectedRows] of [
  ['Jump Direction Determined Last Frame', 10],
  ['Grab During Jumpsquat', 9],
  ['L-Canceling v5.0', 46],
]) {
  const start = annotated.findIndex(line => line.startsWith(prefix));
  assert.ok(start >= 0, prefix);
  let end = start + 1;
  while (end < annotated.length && annotated[end].trim()) end++;
  const block = annotated.slice(start + 1, end)
    .filter(line => line.startsWith('* ')).map(line => line.slice(2));
  assert.equal(block.length, expectedRows, prefix);
  const matches = rows.filter((_, index) => block.every((text, offset) => rows[index + offset]?.text === text));
  assert.equal(matches.length, 1, prefix);
  receipts.push({ title: annotated[start], annotatedLine: start + 1,
    codeRows: block.length, archived36Line: matches[0].line });
}
console.log(JSON.stringify(receipts, null, 2));
