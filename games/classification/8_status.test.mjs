import test from 'node:test';
import assert from 'node:assert/strict';
import { joinStatus, renderRows, MANIFEST } from './8_status.mjs';

const H = 'a'.repeat(64);

function fixture() {
  return {
    catalog: [
      { id: 0, action: 'Wait1', file: 'Wait1.html' },
      { id: 1, action: 'JumpF', file: 'JumpF.html' },
    ],
    phases: [{ name: 'Idle', grounded: true }, { name: 'Jump', grounded: false }],
    phaseAnimation: [
      { phase: 'Idle', action: 0, condition: 'base', axes: [0] },
      { phase: 'Jump', action: 1, condition: 'base', axes: [0] },
    ],
    chartMembership: [
      { phase: 'Idle', ground: true, air: false },
      { phase: 'Jump', ground: false, air: true },
    ],
    chartExempt: [],
    manifestFiles: { 'Wait1.html': H, 'JumpF.html': H },
    manifestFrames: { Wait1: 61, JumpF: 36 },
    hashes: { 'Wait1.html': H, 'JumpF.html': H },
  };
}

// Catalog rows 0..5 with the live conditional selections for airborne attack
// (ID2) and aerial-landing recovery (ID5).
function combatFixture() {
  const names = ['Wait1', 'JumpF', 'AttackAirF', 'JumpSquat', 'Fall', 'LandingAirF'];
  const files = names.map(name => `${name}.html`);
  const base = fixture();
  return {
    ...base,
    catalog: names.map((action, id) => ({ id, action, file: files[id] })),
    phases: [
      { name: 'Idle', grounded: true },
      { name: 'Jump', grounded: false },
      { name: 'Fall', grounded: false },
      { name: 'Landing', grounded: true },
      { name: 'Squat', grounded: true },
    ],
    phaseAnimation: [
      { phase: 'Idle', action: 0, condition: 'base', axes: [0] },
      { phase: 'Jump', action: 1, condition: 'base', axes: [0] },
      { phase: 'Jump', action: 2, condition: 'air_attack', axes: [0] },
      { phase: 'Fall', action: 4, condition: 'base', axes: [0] },
      { phase: 'Fall', action: 2, condition: 'air_attack', axes: [0] },
      { phase: 'Landing', action: 5, condition: 'landing_recovery', axes: [0] },
      { phase: 'Squat', action: 3, condition: 'base', axes: [0] },
    ],
    chartMembership: [
      { phase: 'Idle', ground: true, air: false },
      { phase: 'Jump', ground: false, air: true },
      { phase: 'Fall', ground: false, air: true },
      { phase: 'Landing', ground: true, air: false },
      { phase: 'Squat', ground: true, air: false },
    ],
    manifestFiles: Object.fromEntries(files.map(file => [file, H])),
    manifestFrames: Object.fromEntries(names.map(name => [name, 10])),
    hashes: Object.fromEntries(files.map(file => [file, H])),
  };
}

const codes = result => result.errors.map(error => error.code).sort();

test('the manifest path stays inside the fighter import directory', () => {
  assert.equal(MANIFEST, 'smash/src/fighters/falcon/imported/0_sources.json');
});

test('a clean join prints frames, hash state and selected phases', () => {
  const result = joinStatus(fixture());
  assert.deepEqual(result.errors, []);
  assert.equal(result.rows.length, 2);
  const [wait, jump] = result.rows;
  assert.deepEqual(
    [wait.id, wait.action, wait.frames, wait.hash, wait.selected],
    [0, 'Wait1', 61, 'RETAINED', true],
  );
  assert.deepEqual(wait.phases, ['Idle']);
  assert.deepEqual(wait.chart, ['Idle:ground']);
  assert.deepEqual(jump.phases, ['Jump']);
  assert.deepEqual(jump.chart, ['Jump:air']);
  assert.match(renderRows(result), /catalog 2; selected 2; unselected 0; failures 0/);
});

test('the live runtime overrides select airborne attack ID2 and landing recovery ID5', () => {
  const result = joinStatus(combatFixture());
  assert.deepEqual(result.errors, []);
  const row = id => result.rows[id];
  assert.equal(row(2).selected, true);
  assert.deepEqual(row(2).phases, ['Fall', 'Jump']);
  assert.deepEqual(row(2).chart, ['Fall:air', 'Jump:air']);
  assert.equal(row(5).selected, true);
  assert.deepEqual(row(5).phases, ['Landing']);
  assert.deepEqual(row(5).chart, ['Landing:ground']);
  for (const id of [0, 1, 3, 4]) assert.equal(row(id).selected, true, `action ${id} unselected`);
});

test('duplicate catalog ids, actions and files are rejected', () => {
  const id = fixture();
  id.catalog[1].id = 0;
  assert.match(codes(joinStatus(id)).join(','), /DUPLICATE_ID|CATALOG_ORDER/);

  const name = fixture();
  name.catalog[1].action = 'Wait1';
  assert.match(codes(joinStatus(name)).join(','), /DUPLICATE_NAME/);

  const file = fixture();
  file.catalog[1].file = 'Wait1.html';
  assert.match(codes(joinStatus(file)).join(','), /DUPLICATE_FILE/);
});

test('manifest and hash mismatches are failures, not observations', () => {
  const mismatch = fixture();
  mismatch.hashes['Wait1.html'] = 'b'.repeat(64);
  assert.deepEqual(codes(joinStatus(mismatch)), ['HASH_MISMATCH']);

  const missing = fixture();
  missing.hashes['Wait1.html'] = null;
  assert.deepEqual(codes(joinStatus(missing)), ['MISSING_FILE']);

  const unlisted = fixture();
  unlisted.manifestFiles = { 'JumpF.html': H };
  assert.deepEqual(codes(joinStatus(unlisted)), ['MISSING_MANIFEST']);
});

test('missing phase mapping and out-of-range action indices are rejected', () => {
  const unmapped = fixture();
  unmapped.phases.push({ name: 'Dash', grounded: true });
  assert.deepEqual(codes(joinStatus(unmapped)), ['MISSING_PHASE']);

  const high = fixture();
  high.phaseAnimation[0].action = 2;
  assert.match(codes(joinStatus(high)).join(','), /ACTION_OUT_OF_RANGE/);

  const unknown = fixture();
  unknown.phaseAnimation[0].phase = 'Ghost';
  assert.match(codes(joinStatus(unknown)).join(','), /IMPOSSIBLE_MAPPING/);
});

test('a live mapped phase cannot silently lose its executable chart edge', () => {
  const stripped = fixture();
  stripped.chartMembership = stripped.chartMembership.filter(entry => entry.phase !== 'Jump');
  assert.deepEqual(codes(joinStatus(stripped)), ['CHART_MEMBERSHIP']);

  const empty = fixture();
  empty.chartMembership = empty.chartMembership.map(entry =>
    entry.phase === 'Jump' ? { phase: 'Jump', ground: false, air: false } : entry);
  assert.deepEqual(codes(joinStatus(empty)), ['CHART_MEMBERSHIP']);

  const exempt = fixture();
  exempt.chartMembership = exempt.chartMembership.filter(entry => entry.phase !== 'Jump');
  exempt.chartExempt = ['Jump'];
  assert.deepEqual(codes(joinStatus(exempt)), []);
  assert.match(renderRows(joinStatus(exempt)), /Jump:exempt/);
});
