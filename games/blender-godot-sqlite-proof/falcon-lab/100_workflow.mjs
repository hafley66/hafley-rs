import { execFileSync } from 'node:child_process';
import { fingerprintSources } from '../../shared/workflow/0_fingerprint.mjs';
import { readFileSync, writeFileSync, mkdirSync, mkdtempSync, openSync, closeSync } from 'node:fs';
import { dirname, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { verified } from './94_web/3b_verified.mjs';

const lab = dirname(fileURLToPath(import.meta.url));
const storage = resolve(lab, '.workflow');
const text = (command, args, cwd = lab) => execFileSync(command, args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
const read = path => { try { return JSON.parse(readFileSync(path)); } catch (e) { if (e.code === 'ENOENT') return null; throw e; } };

// Include dirty/untracked non-ignored inputs, locks, fixtures and shared driver.
export function fingerprint() {
  const repo = text('git', ['rev-parse', '--show-toplevel']);
  const sibling = resolve(repo, '../hafley-rs-game-runtime');
  return fingerprintSources([
    [repo, ['.gitmodules', 'AGENTS.md', 'games/AGENTS.md', 'games/shared', 'games/blender-godot-sqlite-proof']],
    [sibling, ['tools/godot-web', 'games/kneeman/app/deploy/scripts']],
  ]);
}

function toolchain() {
  const godot = process.env.GODOT45 || '/Users/chrishafley/godot45/Godot.app/Contents/MacOS/Godot';
  return Object.fromEntries([
    ['node', [process.execPath, '--version']], ['rust', ['rustc', '--version']],
    ['web_rust', ['rustc', '+nightly-2026-05-18', '--version']],
    ['godot', [godot, '--version']], ['ffmpeg', ['ffmpeg', '-version']],
  ].map(([name, [cmd, ...args]]) => [name, text(cmd, args).split('\n')[0]]));
}

export function runStage(directory, name, command, cwd = lab) {
  const log = resolve(directory, name + '.log');
  const fd = openSync(log, 'w');
  const start = Date.now();
  try {
    execFileSync(command[0], command.slice(1), { cwd, stdio: ['ignore', fd, fd] });
    return { name, status: 'passed', elapsed_ms: Date.now() - start, log };
  } catch (error) {
    return { name, status: 'failed', exit_code: error.status, signal: error.signal,
      command, elapsed_ms: Date.now() - start, log,
      error: error.message, excerpt: readFileSync(log, 'utf8').split('\n').slice(-20).join('\n') };
  } finally { closeSync(fd); }
}

export function requireProof(proof, source, tools, artifactHashes) {
  assert.equal(proof?.status, 'passed', 'run just prove first');
  assert.equal(proof.source, source, 'source changed; run just prove');
  assert.deepEqual(proof.tools, tools, 'toolchain changed; run just prove');
  assert.deepEqual(proof.hashes, artifactHashes, 'artifact changed; run just prove');
}

const suites = {
  core: [['core', ['just', '_test-core']]],
  workflow: [['workflow', ['node', '--test', '100_workflow.test.mjs', '94_web/3b_verified.test.mjs']]],
  godot: [['godot', ['just', 'test-godot']], ['controls', ['just', 'test-controls']]],
  web: [['export', ['just', 'web-build']], ['browser', ['just', 'web-verify']]],
};
suites.all = [...suites.core, ...suites.workflow, ...suites.godot];

async function main(command, suite = 'all') {
  if (command === 'status') {
    const source = fingerprint();
    const last = Object.fromEntries(['tsp', 'test', 'prove', 'deploy'].map(name => {
      const receipt = read(resolve(storage, name + '.json'));
      return [name, receipt && { status: receipt.status, commit: receipt.commit, suite: receipt.suite,
        source_current: receipt.source === source, receipt: relative(lab, resolve(storage, name + '.json')),
        mp4: receipt.mp4, url: receipt.url }];
    }));
    console.log(JSON.stringify({ commit: text('git', ['rev-parse', '--short', 'HEAD']),
      dirty: text('git', ['status', '--short']).split('\n').filter(Boolean), last,
      publication: read(resolve(lab, '94_web/build/published.json'))?.url ?? null,
      task: read(resolve(lab, '101_current.json')) }));
    return;
  }
  assert(['tsp', 'test', 'prove', 'deploy'].includes(command), 'usage: just tsp|status|test [suite]|prove|deploy');
  assert(suite in suites, 'unknown test suite');
  mkdirSync(storage, { recursive: true });
  const directory = mkdtempSync(resolve(storage, command + '-'));
  const receipt = { version: 1, command, suite, status: 'running',
    commit: text('git', ['rev-parse', 'HEAD']), source: fingerprint(), stages: [] };
  const save = () => {
    const json = JSON.stringify(receipt, null, 2) + '\n';
    writeFileSync(resolve(directory, 'receipt.json'), json);
    writeFileSync(resolve(storage, command + '.json'), json);
  };
  save();
  try {
    let stages;
    if (command === 'deploy') {
      receipt.tools = toolchain();
      const { receipt: browser } = verified(resolve(lab, '94_web/build/web-game3'));
      requireProof(read(resolve(storage, 'prove.json')), receipt.source, receipt.tools, browser.hashes);
      stages = [['publish', ['just', 'web-publish']]];
    } else if (command === 'prove') {
      receipt.tools = toolchain();
      stages = [...suites.all, ...suites.web];
    } else stages = command === 'tsp' ? [['tsp', ['node', 'contracts/1_generate.mjs']]] : suites[suite];
    for (const [name, args] of stages) {
      console.error(`RUN ${name} log=${relative(lab, resolve(directory, name + '.log'))}`);
      const stage = runStage(directory, name, args);
      receipt.stages.push(stage);
      save();
      assert.equal(stage.status, 'passed', `${name} failed`);
    }
    if (command === 'tsp') receipt.source = fingerprint();
    else assert.equal(fingerprint(), receipt.source, 'source changed during run');
    if (command === 'prove' || (command === 'test' && suite === 'web')) {
      const { receipt: browser } = verified(resolve(lab, '94_web/build/web-game3'));
      receipt.hashes = browser.hashes;
      receipt.mp4 = browser.mp4;
    }
    if (command === 'deploy') Object.assign(receipt, read(resolve(lab, '94_web/build/published.json')));
    receipt.status = 'passed';
  } catch (error) { receipt.status = 'failed'; receipt.error = error.message; process.exitCode = 1; }
  save();
  console.log(JSON.stringify({ command, suite, status: receipt.status,
    stages: receipt.stages.map(s => `${s.name}:${s.status}`),
    receipt: relative(lab, resolve(directory, 'receipt.json')), mp4: receipt.mp4, url: receipt.url,
    error: receipt.error, failure: receipt.stages.find(s => s.status === 'failed') }));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(...process.argv.slice(2)).catch(error => { console.error(error.message); process.exitCode = 1; });
}
