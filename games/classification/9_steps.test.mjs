import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile, mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { root, taskIds, existing, localPath } from './2_registry.mjs';
import { loadSteps, joinSteps, observedFacts } from './9_steps.mjs';

const steps = await loadSteps();
const step = steps['falconFtCommonSourceImport'];
const generated = JSON.parse(await readFile(new URL('./10_steps.json', import.meta.url), 'utf8'));
const sourceRules = JSON.parse(await readFile(
  new URL('../smash/src/fighters/falcon/generated/2_source_rules.json', import.meta.url), 'utf8'));
const AUTHORED_KEYS = ['id', 'task', 'scope', 'sourceKinds', 'extractor', 'artifacts', 'consumer', 'gates', 'stage'];

// One pinned-revision artifact per step is expected; additional instances stay
// in this file as they are authored.
function evidenceFixture() {
  const artifacts = new Map(step.artifacts.map(a => [a.path, { sha256: 'a'.repeat(64), bytes: 10 }]));
  return new Map([[step.id, {
    artifacts,
    observed: { repository: sourceRules.repository, revision: 'c'.repeat(40), rules: 5, unresolved: 3 },
    gates: { 'source-rules-check': 'passed' },
    source: 'b'.repeat(64),
  }]]);
}

function authoredSteps() {
  return structuredClone(steps);
}

test('authored steps declare intent only, with ledger tasks and stages', async () => {
  assert.deepEqual(Object.keys(steps), ['falconFtCommonSourceImport']);
  assert.deepEqual(Object.keys(step), AUTHORED_KEYS);
  const tasks = await taskIds(root);
  assert.ok(tasks.has(step.task), `unknown task ${step.task}`);
  assert.equal(step.id, 'falconFtCommonSourceImport');
  assert.equal(step.stage, 2);
  assert.ok(step.sourceKinds.some(kind => kind.includes('melee-decomp')), 'required source kinds name the pinned decomp');
  assert.match(step.extractor, /smash-import -- falcon/);
  assert.ok(step.consumer.trim().length > 0);
  for (const artifact of step.artifacts) await existing(root, artifact.path);
});

test('authored steps restate no observed revision, hash, count or pass result', () => {
  const text = JSON.stringify(step);
  for (const fact of [sourceRules.revision, /^[0-9a-f]{40}$/, /^[0-9a-f]{64}$/, /\brules\s*[:=]\s*\d/, /passed|failed|stale/i]) {
    assert.doesNotMatch(text, fact instanceof RegExp ? fact : new RegExp(fact.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  }
  for (const artifact of step.artifacts) assert.match(artifact.kind, /^[a-z0-9-]+$/);
});

test('receipt artifacts and observed facts match the generated source rules', () => {
  const receipt = generated.steps[step.id].receipt;
  // UNMEASURED (null) is the honest value when the sibling runtime checkout is
  // not resolvable from this worktree; otherwise a recomputed 64-hex fingerprint.
  assert.ok(receipt.source === null || /^[0-9a-f]{64}$/.test(receipt.source), 'source fingerprint');
  assert.deepEqual(receipt.gates, Object.fromEntries(step.gates.map(gate => [gate.name, 'passed'])));
  const byPath = new Map(receipt.artifacts.map(a => [a.path, a]));
  for (const artifact of step.artifacts) {
    const fact = byMap(byPath, artifact.path);
    assert.equal(fact.kind, artifact.kind);
    assert.match(fact.sha256, /^[0-9a-f]{64}$/);
    assert.ok(fact.bytes > 0);
  }
  assert.equal(receipt.observed.repository, sourceRules.repository);
  assert.equal(receipt.observed.revision, sourceRules.revision);
  assert.equal(receipt.observed.rules, sourceRules.rules.length);
  assert.equal(receipt.observed.unresolved, sourceRules.unresolved.length);
});

function byMap(map, path) { return map.get(path); }

test('the generated file stays compact and shape-locked', () => {
  const raw = execFileSync('node', ['-e',
    `const fs=require('fs');process.stdout.write(String(fs.statSync(process.argv[1]).size))`,
    fileURLToPath(new URL('./10_steps.json', import.meta.url))], { encoding: 'utf8' });
  assert.ok(Number(raw) < 64 * 1024, 'generated JSON must stay below 64 KiB');
  const record = generated.steps[step.id];
  assert.deepEqual(Object.keys(record), ['authored', 'receipt']);
  assert.deepEqual(Object.keys(record.receipt), ['source', 'gates', 'artifacts', 'observed']);
  assert.deepEqual(record.authored, {
    id: step.id, task: step.task, scope: step.scope, sourceKinds: [...step.sourceKinds],
    extractor: step.extractor,
    artifacts: step.artifacts.map(a => ({ kind: a.kind, path: a.path })),
    consumer: step.consumer,
    gates: step.gates.map(g => ({ name: g.name, command: g.command })),
    stage: step.stage,
  });
});

test('the committed receipt is fresh against the working tree and authored gates', async () => {
  execFileSync('node', ['classification/9_steps.mjs', 'check'], {
    cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
  });
});

test('joinSteps rejects missing, extra and contradictory evidence', () => {
  const missing = new Map();
  assert.match(joinSteps(authoredSteps(), missing).errors.map(e => e.code).join(','), /MISSING_RECEIPT/);

  const extra = evidenceFixture();
  extra.get(step.id).artifacts.set('smash/ghost.json', { sha256: 'c'.repeat(64), bytes: 1 });
  assert.match(joinSteps(authoredSteps(), extra).errors.map(e => e.code).join(','), /EXTRA_ARTIFACT/);

  const unauthoredGate = evidenceFixture();
  unauthoredGate.get(step.id).gates.ghost = 'passed';
  assert.match(joinSteps(authoredSteps(), unauthoredGate).errors.map(e => e.code).join(','), /EXTRA_ARTIFACT/);

  const failedGate = evidenceFixture();
  failedGate.get(step.id).gates['source-rules-check'] = 'failed';
  assert.match(joinSteps(authoredSteps(), failedGate).errors.map(e => e.code).join(','), /GATE_FAILED/);

  const unpinned = evidenceFixture();
  unpinned.get(step.id).observed.revision = 'main';
  assert.match(joinSteps(authoredSteps(), unpinned).errors.map(e => e.code).join(','), /CONTRADICTION/);

  const renamed = authoredSteps();
  renamed[step.id].id = 'other';
  assert.match(joinSteps(renamed, evidenceFixture()).errors.map(e => e.code).join(','), /CONTRADICTION|MISSING_RECEIPT/);

  const noFacts = evidenceFixture();
  noFacts.get(step.id).observed = null;
  assert.match(joinSteps(authoredSteps(), noFacts).errors.map(e => e.code).join(','), /MISSING_RECEIPT/);
});

test('observed facts parse only the source-rules artifact kind', () => {
  assert.deepEqual(observedFacts('source-rules-json', JSON.stringify(sourceRules)),
    { repository: sourceRules.repository, revision: sourceRules.revision,
      rules: sourceRules.rules.length, unresolved: sourceRules.unresolved.length });
  assert.equal(observedFacts('other', '{}'), null);
});

test('the checker rejects schema drift and duplicate step identities', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'game-steps-'));
  try {
    const original = await readFile(resolve(root, 'classification/7_steps.tsp'), 'utf8');
    const source = original.replace('./0_model.tsp', resolve(root, 'classification/0_model.tsp'));
    for (const [index, invalid] of [
      source.replace('stage: 2,', 'stage: 9,'),
      source.replace('id: "falconFtCommonSourceImport"', 'ghost: "x"'),
      source.replace('scope: "Falcon common transitions extracted from the pinned Melee decomp"',
        'scope: "Falcon common transitions extracted from the pinned Melee decomp",\n    scope: "duplicate key"'),
      source.replace('gates: #[', 'gatez: #['),
    ].entries()) {
      const path = join(dir, 'invalid.tsp');
      await writeFile(path, invalid);
      await assert.rejects(loadSteps(path), /unexpected-property|unassignable|duplicate|missing-property/, `invalid case ${index}`);
    }
  } finally { await rm(dir, { recursive: true }); }
});

test('step artifact paths stay inside the repository scope', async () => {
  for (const artifact of step.artifacts) localPath(root, artifact.path);
  const escaped = structuredClone(step);
  escaped.artifacts[0].path = '../Cargo.toml';
  await assert.rejects(existing(root, escaped.artifacts[0].path), /escapes scope/);
});
