import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { requireProof, runStage } from './100_workflow.mjs';
import '../../shared/workflow/0_fingerprint.test.mjs';

test('deployment rejects absent, failed, stale-source, stale-toolchain and changed-artifact proofs', () => {
  const proof = { status: 'passed', source: 'source', tools: { rust: 'pinned' }, hashes: { wasm: 'hash' } };
  assert.doesNotThrow(() => requireProof(proof, proof.source, proof.tools, proof.hashes));
  for (const [candidate, source, tools, hashes] of [
    [null, proof.source, proof.tools, proof.hashes],
    [{ ...proof, status: 'failed' }, proof.source, proof.tools, proof.hashes],
    [proof, 'changed', proof.tools, proof.hashes],
    [proof, proof.source, { rust: 'changed' }, proof.hashes],
    [proof, proof.source, proof.tools, { wasm: 'changed' }],
  ]) assert.throws(() => requireProof(candidate, source, tools, hashes));
});

test('stages retain full logs and bounded failure evidence with real exit status', () => {
  const dir = mkdtempSync(join(tmpdir(), 'falcon-workflow-test-'));
  try {
    const pass = runStage(dir, 'pass', [process.execPath, '-e', 'console.log("saved")']);
    assert.equal(pass.status, 'passed');
    assert.equal(readFileSync(pass.log, 'utf8'), 'saved\n');
    const fail = runStage(dir, 'fail', [process.execPath, '-e', 'for(let i=0;i<40;i++) console.error(i); process.exit(7)']);
    assert.deepEqual([fail.status, fail.exit_code, fail.signal], ['failed', 7, null]);
    assert.equal(fail.excerpt.split('\n').length, 20);
    assert.match(readFileSync(fail.log, 'utf8'), /^0\n1\n/);
    assert.match(fail.excerpt, /39\n$/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
