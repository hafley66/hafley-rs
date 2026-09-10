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
      { phase: 'Idle', action: 0, axes: [0] },
      { phase: 'Jump', action: 1, axes: [0] },
    ],
    manifestFiles: { 'Wait1.html': H, 'JumpF.html': H },
    manifestFrames: { Wait1: 61, JumpF: 36 },
    hashes: { 'Wait1.html': H, 'JumpF.html': H },
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
  assert.deepEqual(jump.phases, ['Jump']);
  assert.match(renderRows(result), /catalog 2; selected 2; unselected 0; failures 0/);
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
