import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fingerprintSources } from './0_fingerprint.mjs';

test('fingerprints submodule pins, checkout edits, untracked files and initialization', () => {
  const root = mkdtempSync(join(tmpdir(), 'workflow-gitlink-'));
  const repo = join(root, 'repo');
  const source = join(root, 'source');
  const git = (cwd, ...args) => execFileSync('git', args, { cwd, stdio: 'pipe' }).toString().trim();
  try {
    for (const path of [repo, source]) {
      mkdirSync(path);
      git(path, 'init');
      git(path, 'config', 'user.name', 'Fixture');
      git(path, 'config', 'user.email', 'fixture@example.invalid');
    }
    writeFileSync(join(source, 'asset'), 'first');
    git(source, 'add', 'asset');
    git(source, '-c', 'commit.gpgsign=false', 'commit', '-m', 'fixture');
    const first = git(source, 'rev-parse', 'HEAD');
    git(repo, '-c', 'protocol.file.allow=always', 'submodule', 'add', source, 'dependency');
    const read = () => fingerprintSources([[repo, ['.']]]);
    const clean = read();
    assert.equal(read(), clean);
    const dependency = join(repo, 'dependency');
    writeFileSync(join(dependency, 'asset'), 'changed');
    assert.notEqual(read(), clean);
    writeFileSync(join(dependency, 'asset'), 'first');
    assert.equal(read(), clean);
    writeFileSync(join(dependency, 'extra'), 'new');
    assert.notEqual(read(), clean);
    rmSync(join(dependency, 'extra'));
    writeFileSync(join(source, 'asset'), 'second');
    git(source, 'add', 'asset');
    git(source, '-c', 'commit.gpgsign=false', 'commit', '-m', 'second');
    const second = git(source, 'rev-parse', 'HEAD');
    git(repo, 'update-index', '--cacheinfo', `160000,${second},dependency`);
    assert.notEqual(read(), clean);
    git(repo, 'update-index', '--cacheinfo', `160000,${first},dependency`);
    assert.equal(read(), clean);
    git(repo, 'submodule', 'deinit', '-f', '--', 'dependency');
    assert.notEqual(read(), clean);
    assert.equal(read(), read());
  } finally { rmSync(root, { recursive: true, force: true }); }
});
