import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, copyFile, symlink, readFile, writeFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

test('generation is deterministic, stale checks are read-only, unsupported types fail closed', async () => {
  const root = await mkdtemp(join(tmpdir(), 'falcon-contract-test-'));
  const dir = join(root, 'contracts');
  await mkdir(dir);
  await mkdir(join(root, 'godot'));
  const local = fileURLToPath(new URL('.', import.meta.url));
  const outputs = ['2_presentation_auto.rs', '2_presentation_auto.yaml', '../godot/1_rows_auto.gd'];
  const run = (...args) => spawnSync(process.execPath, ['1_generate.mjs', ...args], { cwd: dir, encoding: 'utf8' });
  try {
    await symlink(join(local, 'node_modules'), join(dir, 'node_modules'), 'dir');
    for (const name of ['0_presentation.tsp', '0_constants.mjs', '0_rows.mjs', '1_rows.mjs', '1_generate.mjs']) await copyFile(join(local, name), join(dir, name));
    let result = run();
    assert.equal(result.status, 0, result.stderr);
    const expected = await Promise.all(outputs.map(name => readFile(join(local, name), 'utf8')));
    assert.deepEqual(await Promise.all(outputs.map(name => readFile(join(dir, name), 'utf8'))), expected);
    for (const [mutation, error] of [
      [source => source.replace('@row(3)', '@row(2)'), /invalid\/duplicate row kind/],
      [source => source.replace('radius: float64;', 'radius: uint32;'), /unsupported packed field/],
      [source => source.replace('model AttackValues {', 'model AttackValues {\n @minItems(24) @maxItems(24) overflow: float64[];'), /packed row exceeds/],
    ]) {
      const source = await readFile(join(local, '0_presentation.tsp'), 'utf8');
      await writeFile(join(dir, '0_presentation.tsp'), mutation(source));
      const rejected = run();
      assert.notEqual(rejected.status, 0);
      assert.match(rejected.stderr, error);
      assert.deepEqual(await Promise.all(outputs.map(name => readFile(join(dir, name), 'utf8'))), expected);
    }
    await copyFile(join(local, '0_presentation.tsp'), join(dir, '0_presentation.tsp'));
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
    await writeFile(join(dir, '0_presentation.tsp'), source + '\nconst MAX_ID: uint64 = 18446744073709551615;\nconst CAPACITY_ALIAS: uint32 = ROW_CAPACITY;\nconst LABEL: string = "two knees";\n');
    result = run();
    assert.equal(result.status, 0, result.stderr);
    const numericOutput = await readFile(join(dir, outputs[0]), 'utf8');
    assert.match(numericOutput, /MAX_ID: u64 = 18446744073709551615;/);
    assert.match(numericOutput, /CAPACITY_ALIAS: u32 = 1024;/);
    assert.match(numericOutput, /LABEL: &'static str = "two knees";/);
    await writeFile(join(dir, '0_presentation.tsp'), source + '\nconst BAD: uint32 = -1;\n');
    assert.notEqual(run().status, 0);
    assert.equal(await readFile(join(dir, outputs[0]), 'utf8'), numericOutput);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
