import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { root, loadRegistry, validateRegistry, cargoMetadata, localPath, renderD2, output, taskIds, checkSvg } from './2_registry.mjs';

const entries = await loadRegistry();
const cache = new Map();
const metadata = path => {
  if (!cache.has(path)) cache.set(path, cargoMetadata(path));
  return cache.get(path);
};

test('native TSP values resolve all existing first-party Cargo packages', async () => {
  await validateRegistry(entries, root, metadata);
  assert.deepEqual(Object.entries(entries).map(([name, e]) => [name, e.stage, e.destination, Boolean(e.manifest)]), [
    ['game-content', 2.7, 'library', true],
    ['redux', 3, 'library', true], ['game-input', 3, 'library', true],
    ['rollback', 3, 'library', true], ['game-capture', 3, 'library', true],
    ['core-labs', 2.7, 'split', true],
    ['trace-gpu-host', 2.7, 'split', true], ['falcon-lab', 2.7, 'split', true],
    ['falcon_web', 2.7, 'split', true], ['smash', 2.7, 'app', true],
  ]);
});

test('repository checks reject broken identity, references and promotion claims', async () => {
  const cases = [
    [e => { e.wrong = e.redux; delete e.redux; }, /Cargo package mismatch/],
    [e => { e.redux.manifest = 'shared/absent/Cargo.toml'; }, /ENOENT/],
    [e => { e.redux.evidence = ['missing-proof.md']; }, /ENOENT/],
    [e => { e.redux.evidence = ['shared']; }, /expected file/],
    [e => { e.redux.evidence = []; }, /qualification requires evidence/],
    [e => { e.redux.task = 'NEVER'; }, /unknown task/],
    [e => { e.redux.task = 'A2, A3'; }, /unknown task/],
    [e => { e.redux.stage = 4; e.redux.targets = ['crates/pending-redux']; }, /stage 4 requires integration/],
    [e => { e.redux.targets = ['smash/crates/redux']; }, /library targets/],
    [e => { e.smash.stage = 2; delete e.smash.manifest; }, /implemented stage requires manifest/],
    [e => { e.smash.targets = ['crates/smash']; }, /app target/],
    [e => { e['core-labs'].targets = ['crates/physics']; }, /split requires/],
    [e => { e.redux.evidence = ['../Cargo.toml']; }, /escapes scope/],
    [e => { delete e.redux; }, /unclassified Cargo manifests/],
  ];
  for (const [mutate, pattern] of cases) {
    const candidate = structuredClone(entries);
    mutate(candidate);
    await assert.rejects(validateRegistry(candidate, root, metadata), pattern);
  }
});

test('compiler and registry guard reject invalid schema and duplicate identities', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'game-classification-'));
  try {
    const original = await readFile(resolve(root, 'classification/1_registry.tsp'), 'utf8');
    const source = original.replace('./0_model.tsp', resolve(root, 'classification/0_model.tsp'));
    for (const [index, invalid] of [
      source.replace('stage: 3', 'stage: 9'),
      source.replace('destination: "library"', 'destination: "engine"'),
      source.replace('scope:', 'scpoe:'),
      source.replace('  rollback: #{', '  redux: #{'),
      source.replace(': Record<Entry>', '').replace('stage: 3', 'stage: 9'),
      source.replace(': Record<Entry>', '').replace('destination: "library"', 'destination: "invalid"'),
      source.replace('const entries:', 'const data:').replace('  rollback: #{', '  redux: #{') + '\nconst entries: Record<Entry> = data;\n',
    ].entries()) {
      const path = join(dir, 'invalid.tsp');
      await writeFile(path, invalid);
      await assert.rejects(loadRegistry(path), /unassignable|missing-property|unexpected-property|duplicate|inline/, `invalid case ${index}`);
    }
    const untyped = source.replace(': Record<Entry>', '');
    const validPath = join(dir, 'valid.tsp');
    await writeFile(validPath, untyped);
    assert.deepEqual(await loadRegistry(validPath), entries);
    const missingModel = join(dir, 'missing-model.tsp');
    await writeFile(missingModel, 'const entries = #{ghost: #{stage: 1, destination: "invalid", scope: "fixture", task: "A1", targets: #[], evidence: #[]}};');
    await assert.rejects(loadRegistry(missingModel), /unassignable/);
  } finally { await rm(dir, { recursive: true }); }
});

test('task references use ID tables in the latest ledger and two predecessors', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'game-task-ledgers-'));
  try {
    for (const n of [1, 3, 9, 10]) {
      await writeFile(join(dir, `${n}_tasks.md`), `| ID | State | Scope |\n| --- | --- | --- |\n| A${n} | Queued | Work |\n\n| Tasks | Destination |\n| A2, A3 | app |\n| FAKE1 | library |\n`);
    }
    assert.deepEqual([...await taskIds(dir)].sort(), ['A10', 'A3', 'A9']);
  } finally { await rm(dir, { recursive: true }); }
});

test('SVG freshness check rejects corruption without overwriting it', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'game-svg-test-'));
  try {
    const d2 = join(dir, '1_roadmap.d2');
    const svg = join(dir, '1_roadmap.svg');
    await writeFile(d2, 'a -> b\n');
    execFileSync('d2', ['--layout', 'elk', '--pad', '24', d2, svg], { stdio: 'pipe', timeout: 20000 });
    await checkSvg(dir);
    await writeFile(svg, 'corrupt');
    await assert.rejects(checkSvg(dir), /stale generated output/);
    assert.equal(await readFile(svg, 'utf8'), 'corrupt');
  } finally { await rm(dir, { recursive: true }); }
});

test('paths stay scoped and generated checks detect staleness without writing', async () => {
  for (const path of ['/tmp/x', '../x', 'x/../../y', '.', 'x\\y']) {
    assert.throws(() => localPath(root, path), /relative path|escapes scope/);
  }
  const dir = await mkdtemp(join(tmpdir(), 'game-classification-output-'));
  try {
    const path = join(dir, 'out');
    await output(path, 'original', false);
    await assert.rejects(output(path, 'changed', true), /stale generated output/);
    assert.equal(await readFile(path, 'utf8'), 'original');
  } finally { await rm(dir, { recursive: true }); }
  const d2 = renderD2(entries);
  assert.equal(d2, await readFile(new URL('3_registry.d2', import.meta.url), 'utf8'));
  const proposal = structuredClone(entries);
  proposal.smash.stage = 1;
  delete proposal.smash.manifest;
  assert.match(renderD2(proposal), /PROPOSAL · crate absent/);
  assert.match(d2, /2\.7 · Testing/);
});
