import test from 'node:test';
import assert from 'node:assert/strict';
import { COLUMNS, joinStatus, loadStatus, receiptObservation, currentSource } from './8_status.mjs';

const authored = await loadStatus();
const H = 'a'.repeat(64);

function fixture() {
  return {
    status: { expected: {
      A: { id: 0, action: 'A', family: 'Fam' },
      B: { id: 1, action: 'B', family: 'Fam' },
    } },
    axes: [...COLUMNS],
    catalog: [
      { id: 0, action: 'A', file: 'A.html' },
      { id: 1, action: 'B', file: 'B.html' },
    ],
    phases: [{ name: 'Idle', grounded: true }, { name: 'Dash', grounded: true }],
    phaseAnimation: [
      { phase: 'Idle', action: 0, axes: [0] },
      { phase: 'Dash', action: 1, axes: [0] },
    ],
    ground: [{ from: 'Idle', event: 'Motion', to: 'Dash', witnesses: 1 }],
    air: [{ from: 'Dash', event: 'Land', to: null, witnesses: 1 }],
    ingestRows: [
      { action: 'A', file: 'A.html', state: 'retained', hash: H, expected: H, frames: 1 },
      { action: 'B', file: 'B.html', state: 'retained', hash: H, expected: H, frames: 2 },
    ],
    mechanics: { fam: { family: 'Fam', fidelity: 'unqualified' } },
    restore: 'STALE',
    native: 'STALE',
  };
}

const codes = result => result.errors.map(error => error.code).sort();

// Full authored catalog joined with the live selection seam: the 13 base-pose
// action IDs plus the conditional airborne attack (ID2) and aerial-landing
// recovery (ID5) runtime overrides. Mirrors movement::select over the export.
const SELECTED_IDS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 14, 16];

function catalogFixture() {
  const entries = Object.values(authored.expected).sort((a, b) => a.id - b.id);
  return {
    status: authored,
    axes: [...COLUMNS],
    catalog: entries.map(e => ({ id: e.id, action: e.action, file: `${e.action}.html` })),
    phases: entries.map(e => ({ name: e.action, grounded: true })),
    phaseAnimation: entries
      .filter(e => SELECTED_IDS.includes(e.id))
      .map(e => ({
        phase: e.action,
        action: e.id,
        condition: [2, 5].includes(e.id) ? 'conditional' : 'base',
        axes: [0],
      })),
    ground: [],
    air: [],
    ingestRows: entries.map(e => ({
      action: e.action, file: `${e.action}.html`, state: 'retained', hash: H, expected: H, frames: 1,
    })),
    mechanics: Object.fromEntries(
      [...new Set(entries.map(e => e.family))].map((family, index) => [
        `m${index}`, { family, fidelity: 'unqualified' },
      ])),
    restore: 'STALE',
    native: 'STALE',
  };
}

test('authored status declares the 22 stable actions and the required axes', () => {
  assert.deepEqual(authored.axes, COLUMNS);
  assert.equal(Object.keys(authored.expected).length, 22);
  const ids = Object.values(authored.expected).map(entry => entry.id).sort((a, b) => a - b);
  assert.deepEqual(ids, Array.from({ length: 22 }, (_, index) => index));
  assert.equal(authored.expected.AttackAirF.id, 2);
  assert.equal(authored.expected.JumpAerialB.id, 21);
});

test('success join maps payload, phase, chart, live and fidelity separately', () => {
  const result = joinStatus(fixture());
  assert.deepEqual(result.errors, []);
  assert.equal(result.rows.length, 2);
  const [a, b] = result.rows;
  assert.equal(a.payload.state, 'retained');
  assert.deepEqual(a.phases, ['Idle']);
  assert.equal(a.chart, 1);
  assert.equal(a.live, true);
  assert.equal(a.fidelity, 'unqualified');
  assert.deepEqual(b.phases, ['Dash']);
  assert.equal(b.live, true);
});

test('the authored catalog joins to 15 selected and 7 unselected actions', () => {
  const result = joinStatus(catalogFixture());
  assert.deepEqual(result.errors, []);
  const selected = result.rows.filter(row => row.live).map(row => row.id);
  assert.deepEqual(selected, SELECTED_IDS);
  assert.equal(selected.length, 15);
  assert.deepEqual(result.rows.filter(row => !row.live).map(row => row.id), [13, 15, 17, 18, 19, 20, 21]);
  for (const id of [2, 5]) assert.equal(result.rows[id].live, true, `action ${id} unselected`);
});

test('duplicate, missing and extra stable ids are rejected', () => {
  const duplicate = fixture();
  duplicate.status.expected.B.id = 0;
  assert.match(codes(joinStatus(duplicate)).join(','), /DUPLICATE_ID/);

  const missing = fixture();
  missing.catalog = [missing.catalog[0]];
  assert.match(codes(joinStatus(missing)).join(','), /MISSING_STABLE_ID/);

  const extra = fixture();
  extra.catalog.push({ id: 2, action: 'Ghost', file: 'Ghost.html' });
  assert.match(codes(joinStatus(extra)).join(','), /CATALOG_ORDER|EXTRA_CATALOG/);

  const renamed = fixture();
  renamed.catalog[1].action = 'Z';
  assert.match(codes(joinStatus(renamed)).join(','), /CONTRADICTION/);
});

test('broken hashes and missing payloads are failures, not observations', () => {
  const broken = fixture();
  broken.ingestRows[0].state = 'mismatch';
  assert.deepEqual(codes(joinStatus(broken)), ['BROKEN_HASH']);

  const absent = fixture();
  absent.ingestRows = [absent.ingestRows[1]];
  assert.match(codes(joinStatus(absent)).join(','), /MISSING_PAYLOAD/);
});

test('impossible phase/action mappings are rejected', () => {
  const unknownAction = fixture();
  unknownAction.phaseAnimation.push({ phase: 'Idle', action: 99, axes: [0] });
  assert.match(codes(joinStatus(unknownAction)).join(','), /IMPOSSIBLE_MAPPING/);

  const unknownPhase = fixture();
  unknownPhase.phaseAnimation.push({ phase: 'Ghost', action: 0, axes: [0] });
  assert.match(codes(joinStatus(unknownPhase)).join(','), /IMPOSSIBLE_MAPPING/);

  const unknownTransition = fixture();
  unknownTransition.ground.push({ from: 'Idle', event: 'Motion', to: 'Ghost', witnesses: 1 });
  assert.match(codes(joinStatus(unknownTransition)).join(','), /IMPOSSIBLE_MAPPING/);
});

test('axes and fidelity contradictions are rejected', () => {
  const axes = fixture();
  axes.axes = COLUMNS.slice(0, -1);
  assert.match(codes(joinStatus(axes)).join(','), /AXES/);

  const family = fixture();
  family.status.expected.A.family = 'Absent';
  assert.match(codes(joinStatus(family)).join(','), /MISSING_STABLE_ID/);

  const conflicting = fixture();
  conflicting.mechanics.other = { family: 'Fam', fidelity: 'qualified' };
  assert.match(codes(joinStatus(conflicting)).join(','), /CONTRADICTION/);
});

test('receipts are observed only against a matching source fingerprint', () => {
  assert.equal(receiptObservation(null, H, ['core']), 'UNMEASURED');
  assert.equal(receiptObservation({}, null, ['core']), 'UNVERIFIED, source not recomputable');
  assert.equal(receiptObservation({ source: 'b', status: 'passed' }, H, ['core']), 'STALE');
  assert.equal(receiptObservation({ source: H, status: 'failed' }, H, ['core']), 'FAILED');
  const passed = { source: H, status: 'passed', stages: [{ name: 'core', status: 'passed' }] };
  assert.equal(receiptObservation(passed, H, ['core']), 'PASSED');
  assert.equal(receiptObservation(passed, H, ['export']), 'FAILED');
});

test('the current source fingerprint recomputes to a stable digest', () => {
  const source = currentSource();
  assert.match(source, /^[0-9a-f]{64}$/);
  assert.equal(source, currentSource());
});
