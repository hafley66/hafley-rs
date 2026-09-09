import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { root, loadRegistry, validateRegistry, cargoMetadata, localPath, renderD2, output } from './2_registry.mjs';

const entries = await loadRegistry();
const cache = new Map();
const metadata = path => {
  if (!cache.has(path)) cache.set(path, cargoMetadata(path));
  return cache.get(path);
};

test('native TSP values resolve all existing first-party Cargo packages', async () => {
  await validateRegistry(entries, root, metadata);
  assert.deepEqual(Object.entries(entries).map(([name, e]) => [name, e.stage, e.destination, Boolean(e.manifest)]), [
    ['redux', 3, 'library', true], ['game-input', 3, 'library', true],
    ['rollback', 3, 'library', true], ['game-capture', 3, 'library', true],
    ['falcon-simulation', 2.7, 'split', true], ['core-labs', 2.7, 'split', true],
    ['trace-gpu-host', 2.7, 'split', true], ['falcon-lab', 2.7, 'split', true],
    ['falcon_web', 2.7, 'split', true], ['smash', 1, 'app', false],
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
    [e => { e.redux.stage = 4; }, /stage 4 requires integration/],
    [e => { e.redux.targets = ['smash/crates/redux']; }, /library targets/],
    [e => { e.smash.stage = 2; }, /implemented stage requires manifest/],
    [e => { e.smash.targets = ['crates/smash']; }, /app target/],
    [e => { e['falcon-simulation'].targets = ['crates/simulation']; }, /split requires/],
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
    ].entries()) {
      const path = join(dir, 'invalid.tsp');
      await writeFile(path, invalid);
      await assert.rejects(loadRegistry(path), /unassignable|missing-property|unexpected-property|duplicate/, `invalid case ${index}`);
    }
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
  assert.match(d2, /PROPOSAL · crate absent/);
  assert.match(d2, /2\.7 · Testing/);
});
