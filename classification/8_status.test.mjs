import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { COLUMNS, joinStatus, loadStatus, receiptObservation, currentSource,
  buildStatusProjection, checkStatusProjection, renderCharacterMatrix } from './8_status.mjs';

const authored = await loadStatus();
const H = 'a'.repeat(64);

// Pure inputs for the `just status` SOURCE RULES section. These are small JSON
// records, not the generated HTML/SVG or the base64 payloads.
const sourceRules = JSON.parse(await readFile(
  new URL('../smash/src/fighters/pigeon/generated/2_source_rules.json', import.meta.url), 'utf8'));
const manifest = JSON.parse(await readFile(
  new URL('../smash/src/fighters/pigeon/imported/0_sources.json', import.meta.url), 'utf8'));
const SOURCE_OPS = ['Less', 'LessEqual', 'Greater', 'GreaterEqual', 'Equal', 'NotEqual'];

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

// Full authored catalog joined with the live selection seam: the 16 base-pose
// action IDs (including the crouch lifecycle 18/19/20) plus the conditional
// airborne attack (ID2) and aerial-landing recovery (ID5) runtime overrides.
// Mirrors movement::select over the export.
const SELECTED_IDS = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 14, 16, 18, 19, 20];

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

test('the authored catalog joins to 18 selected and 4 unselected actions', () => {
  const result = joinStatus(catalogFixture());
  assert.deepEqual(result.errors, []);
  const selected = result.rows.filter(row => row.live).map(row => row.id);
  assert.deepEqual(selected, SELECTED_IDS);
  assert.equal(selected.length, 18);
  assert.deepEqual(result.rows.filter(row => !row.live).map(row => row.id), [13, 15, 17, 21]);
  for (const id of [2, 5, 18, 19, 20]) assert.equal(result.rows[id].live, true, `action ${id} unselected`);
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

test('dog status projects 25 rows from checked generated inputs', async () => {
  const projection = await buildStatusProjection();
  const pigeon = projection.characters.find(character => character.key === 'pigeon');
  const dog = projection.characters.find(character => character.key === 'dog');
  assert.equal(pigeon.catalog.count, 22);
  assert.equal(pigeon.rows.length, 22);
  assert.equal(dog.catalog.count, 25);
  assert.equal(dog.rows.length, 25);
  assert.deepEqual(dog.errors, []);
  assert.deepEqual(dog.rows.map(row => row.id), Array.from({ length: 25 }, (_, index) => index));
  // The generated role artifact binds all 19 roles to 19 exact Dog actions; the
  // 6 unbound rows are catalog actions with no generated role binding. Binding
  // is not a runtime phase, so phase and live stay explicit UNKNOWN/unmeasured.
  const bound = dog.rows.filter(row => row.bound).map(row => row.id);
  const unbound = dog.rows.filter(row => !row.bound).map(row => row.id);
  assert.deepEqual(bound, [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 17, 18, 19, 20, 21, 22, 23, 24]);
  assert.equal(bound.length, 19);
  assert.deepEqual(unbound, [9, 10, 11, 13, 14, 15]);
  assert.equal(unbound.length, 6);
  assert.deepEqual(dog.rows[0].bindings, ['Idle']);
  assert.equal(dog.rows[9].bindings.length, 0);
  assert.equal(dog.rows.filter(row => row.live).length, 0);
  assert.ok(dog.rows.every(row => row.phase === 'UNKNOWN' && row.live === false));
  assert.deepEqual(projection.characters.map(character => character.rows.length), [22, 25]);
});

test('character matrices stay separated and keep unknown axes explicit', async () => {
  const projection = await buildStatusProjection();
  const dog = projection.characters.find(character => character.key === 'dog');
  const text = renderCharacterMatrix(dog);
  assert.equal(text.split('\n').length, 2 + dog.rows.length + 1);
  assert.match(text, /bind\s+phase/);
  assert.match(text, /FAILURES: none/);
  assert.match(text, /UNMEASURED/);
  assert.match(text, /UNKNOWN/);
  assert.doesNotMatch(text, /%/);
});

test('the committed status projection is rejected when a generated input or row is stale', async () => {
  const live = await buildStatusProjection();
  assert.deepEqual(await checkStatusProjection(live), live);
  const rowDrift = structuredClone(live);
  rowDrift.characters[1].rows[0].action = 'Ghost';
  await assert.rejects(checkStatusProjection(rowDrift), /stale status projection/);
  const inputDrift = structuredClone(live);
  inputDrift.characters[1].catalog.sha256 = 'f'.repeat(64);
  await assert.rejects(checkStatusProjection(inputDrift), /stale status projection/);
});

// The SOURCE RULES section prints every extracted rule and each unresolved
// value. Assert its input shape: one pinned decomp revision, a line anchor per
// rule, and no fabricated numeric values.
test('source rules pin one decomp revision with line-anchored guards', () => {
  assert.equal(sourceRules.repository, 'https://github.com/doldecomp/melee.git');
  assert.match(sourceRules.revision, /^[0-9a-f]{40}$/);
  assert.equal(sourceRules.rules.length, 5);
  for (const rule of sourceRules.rules) {
    assert.ok(rule.from && rule.event && rule.to, 'rule needs from/event/to');
    assert.equal(rule.source.repository, sourceRules.repository);
    assert.equal(rule.source.revision, sourceRules.revision);
    assert.match(rule.source.path, /^src\/melee\/ft\//);
    assert.ok(rule.source.line > 0, `${rule.source.symbol} needs a nonzero line anchor`);
    assert.ok(rule.source.symbol.length > 0);
    assert.ok(SOURCE_OPS.includes(rule.guard.operator), `unknown operator ${rule.guard.operator}`);
    assert.ok(rule.guard.lhs && rule.guard.rhs);
  }
  assert.deepEqual(
    sourceRules.rules.map(rule => `${rule.from}-${rule.event}->${rule.to}`),
    [
      'Wait-turn_request->Turn',
      'KneeBend-takeoff->JumpF',
      'KneeBend-takeoff->JumpB',
      'Fall-air_jump->JumpAerialF',
      'Fall-air_jump->JumpAerialB',
    ]);
});

test('unresolved source values stay explicit and name their missing input', () => {
  assert.deepEqual(
    sourceRules.unresolved.map(item => item.symbol),
    ['p_ftCommonData->x34', 'p_ftCommonData->x78', 'LandingLight selection']);
  for (const item of sourceRules.unresolved) {
    assert.ok(item.reason.trim().length > 0, `${item.symbol} needs a reason`);
  }
  assert.equal(sourceRules.unresolved[0].source.revision, sourceRules.revision);
  assert.equal(sourceRules.unresolved[1].source.revision, sourceRules.revision);
  assert.match(sourceRules.unresolved[0].reason, /no retained DAT input/);
  assert.match(sourceRules.unresolved[2].reason, /no retained PM selection rule/);
  assert.equal(sourceRules.unresolved[2].source.repository, 'https://rukaidata.com/PM3.6/Captain%20Falcon/subactions/');
  assert.equal(manifest.frames.LandingLight, 3);
});

test('the retained manifest records the source boundary', () => {
  const { boundary } = manifest;
  assert.match(boundary.generator, /Rukaidata GitHub .* generator\/parser source/);
  assert.match(boundary.artifacts, /base64\+bincode/);
  assert.match(boundary.raw_inputs, /PAC\/GCT inputs are absent/);
  assert.match(boundary.transitions, /github\.com\/doldecomp\/melee submodule/);
  assert.match(boundary.transitions, /c7861544f8e1fbc530612393e91d859886e97e3c/);
});
