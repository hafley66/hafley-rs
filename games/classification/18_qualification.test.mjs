import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { generate } from './18_qualification.mjs';

test('generated animation cases retain four callbacks and eight ordered calls', async () => {
  const cases = await generate();
  const inventory = JSON.parse(await readFile(new URL('12_source_inventory.json', import.meta.url), 'utf8'));
  assert.equal(cases.callbacks.length, 4);
  assert.equal(cases.callbacks.flatMap(row => row.ordered_calls).length, 8);
  for (const row of cases.callbacks) {
    const source = inventory.callbacks.find(callback => callback.id === row.source_callback_id);
    assert.ok(source);
    assert.equal(row.source.revision, cases.source_revision);
    assert.deepEqual(row.ordered_calls.map(call => call.ordinal), [0, 1]);
    assert.deepEqual(row.ordered_calls.map(call => call.id), source.calls.map(call => call.id));
    assert.equal(row.source_completion.qualification, 'unqualified');
  }
});

test('qualification findings remain explicit and contribute no receipts', async () => {
  const cases = await generate();
  for (const row of cases.callbacks) assert.equal(row.source_completion.qualification, 'unqualified');
  const receipts = JSON.parse(await readFile(new URL('20_qualification_receipts.json', import.meta.url), 'utf8'));
  assert.equal(receipts.receipts.length, 0);
  assert.deepEqual(receipts.findings.map(row => row.status), ['unqualified', 'unqualified', 'unqualified', 'unqualified']);
});
