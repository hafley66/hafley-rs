import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, copyFile, symlink, readFile, writeFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

test('generation is deterministic, stale checks are read-only, unsupported types fail closed', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'falcon-contract-test-'));
  const local = fileURLToPath(new URL('.', import.meta.url));
  const outputs = ['2_presentation_auto.rs', '2_presentation_auto.yaml'];
  const run = (...args) => spawnSync(process.execPath, ['1_generate.mjs', ...args], { cwd: dir, encoding: 'utf8' });
  try {
    await symlink(join(local, 'node_modules'), join(dir, 'node_modules'), 'dir');
    for (const name of ['0_presentation.tsp', '1_generate.mjs']) await copyFile(join(local, name), join(dir, name));
    let result = run();
    assert.equal(result.status, 0, result.stderr);
    const expected = await Promise.all(outputs.map(name => readFile(join(local, name), 'utf8')));
    assert.deepEqual(await Promise.all(outputs.map(name => readFile(join(dir, name), 'utf8'))), expected);
    const timestamps = await Promise.all(outputs.map(name => stat(join(dir, name)).then(s => s.mtimeMs)));
    result = run();
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(await Promise.all(outputs.map(name => stat(join(dir, name)).then(s => s.mtimeMs))), timestamps);
    assert.equal(run('--check').status, 0);
    const source = await readFile(join(dir, '0_presentation.tsp'), 'utf8');
    await writeFile(join(dir, '0_presentation.tsp'), source.replace('tick: int64;', 'tick: uint32;'));
    result = run('--check');
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /stale generated file/);
    assert.deepEqual(await Promise.all(outputs.map(name => readFile(join(dir, name), 'utf8'))), expected);
    await writeFile(join(dir, '0_presentation.tsp'), source.replace('tick: int64;', 'tick: string;'));
    result = run();
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /unsupported boundary type Scalar:string/);
    assert.deepEqual(await Promise.all(outputs.map(name => readFile(join(dir, name), 'utf8'))), expected);
    await writeFile(join(dir, '0_presentation.tsp'), source.replace('@minItems(24) @maxItems(24)', '@minItems(1) @maxItems(24)'));
    result = run();
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /arrays require equal positive minItems\/maxItems/);
    assert.deepEqual(await Promise.all(outputs.map(name => readFile(join(dir, name), 'utf8'))), expected);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
