import test from 'node:test';
import assert from 'node:assert/strict';
import { AXES, joinCoverage, renderCoverage } from './15_coverage.mjs';

const H = 'a'.repeat(64);
const sourceState = (id, name) => ({ id, name, ordinal: Number(id), source: { symbol: name } });
const callback = (id, name) => ({ id, action: name, phase: 'Anim', source: { symbol: name }, calls: [] });

function fixture() {
  return {
    port: {
      profile: {
        id: 'fixture', name: 'Fixture', target: 'test', sourceInventory: 'source',
        runtimeInventory: 'runtime', sourceRevision: 'rev', axes: [...AXES],
      },
      mappings: [
        { sourceId: 'a', runtimeState: 'Idle', reason: 'neutral', evidence: ['source'] },
        { sourceId: 'b', runtimeState: 'Idle', reason: 'neutral band', evidence: ['source'] },
      ],
      exclusions: [{ id: 'items', namePrefixes: ['Item'], reason: 'outside profile', evidence: ['source'] }],
    },
    source: {
      ruleset: 'fixture', repository: 'source', revision: 'rev',
      states: [sourceState('a', 'A'), sourceState('b', 'B'), sourceState('item', 'ItemCard'), sourceState('c', 'C')],
      callbacks: [callback('cb1', 'callback-one'), callback('cb2', 'callback-two')],
      counts: { states: 4, callbacks: 2, direct_calls: 3, unresolved: 0 },
    },
    runtime: {
      profile: 'fixture', source_revision: 'rev', runtime_revision: 'runtime-rev', source_fingerprint: H,
      runtime: { states: [{ name: 'Idle' }], ground: [], air: [] },
    },
    receipt: { source: 'old', status: 'passed', stages: [{ name: 'core', status: 'passed' }] },
    fingerprint: H,
    runtimeRevision: 'runtime-rev',
  };
}

test('coverage has all six required independent axes and exact fractions', () => {
  const result = joinCoverage(fixture());
  assert.deepEqual(Object.keys(result.axes), AXES);
  assert.deepEqual(result.axes['state mapping'], { numerator: 2, denominator: 3, percentage: 66.67 });
  assert.deepEqual(result.axes['transition/callback mapping'], { numerator: 0, denominator: 2, percentage: 0 });
  assert.deepEqual(result.axes['ordered guard qualification'], { numerator: 0, denominator: 3, percentage: 0 });
  assert.equal(result.strict.numerator, 0);
});

test('adding an unresolved source state grows the denominator and lowers coverage', () => {
  const before = joinCoverage(fixture());
  const afterInput = fixture();
  afterInput.source.states.push(sourceState('d', 'D'));
  afterInput.source.counts.states += 1;
  const after = joinCoverage(afterInput);
  assert.equal(before.axes['state mapping'].denominator, 3);
  assert.equal(after.axes['state mapping'].denominator, 4);
  assert.ok(after.axes['state mapping'].percentage < before.axes['state mapping'].percentage);
  assert.ok(after.unresolved.states.some(row => row.source_id === 'd'));
});

test('omitted mappings remain unresolved', () => {
  const result = joinCoverage(fixture());
  assert.deepEqual(result.unresolved.states.map(row => row.source_id), ['c']);
});

test('stale receipts contribute zero to rollback and source fidelity', () => {
  const result = joinCoverage(fixture());
  assert.equal(result.receipts.rollback, 'STALE');
  assert.equal(result.axes.rollback.numerator, 0);
  assert.equal(result.axes['source fidelity'].numerator, 0);
});

test('many source states mapped to one runtime state remain separate rows', () => {
  const result = joinCoverage(fixture());
  assert.equal(result.axes['state mapping'].numerator, 2);
  assert.deepEqual(result.exclusions.rows.map(row => row.source_id), ['item']);
  assert.equal(result.exclusions.rules.items, 1);
});

test('authored mappings alone cannot raise observed fidelity', () => {
  const result = joinCoverage(fixture());
  assert.equal(result.axes['state mapping'].numerator, 2);
  assert.equal(result.axes['source fidelity'].numerator, 0);
  assert.match(renderCoverage(result), /profile: fixture/);
  assert.match(renderCoverage(result), /source revision: rev/);
});

test('shared callback identities preserve every action association', () => {
  const input = fixture();
  input.source.callbacks = [{ id: 'shared', actions: ['A', 'B'], phase: 'Anim', source: { symbol: 'shared' }, calls: [] }];
  input.source.counts.callbacks = 1;
  input.runtime.runtime.callbacks = [{ id: 'Motion' }];
  input.port.callbackMappings = [{ sourceId: 'shared', runtimeCallback: 'Motion', reason: 'shared event', evidence: ['runtime'] }];
  const result = joinCoverage(input);
  assert.deepEqual(result.axes['transition/callback mapping'], { numerator: 2, denominator: 2, percentage: 100 });
  assert.equal(result.unresolved.callbacks.length, 0);
});
