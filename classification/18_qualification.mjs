import { readFile, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { currentRevision, currentSourceFingerprint } from './15_coverage.mjs';

const here = fileURLToPath(new URL('.', import.meta.url));
const sourcePath = resolve(here, '12_source_inventory.json');
const outputPath = resolve(here, '19_qualification_cases.json');
const receiptsPath = resolve(here, '20_qualification_receipts.json');
const sourceRoot = resolve(here, '../vendor/melee');
const sourceRevision = 'c7861544f8e1fbc530612393e91d859886e97e3c';
const targetCallbacks = [
  ['ftCo_Dash_Anim', 'Dash', 'Dash', true, 'Run', 'ftCo_MS_Wait', 'contextual ft_8008A2BC destination differs from runtime dash completion (Run/Brake)'],
  ['ftCo_Landing_Anim', 'Landing', 'Landing', false, 'Idle', 'ftCo_MS_Wait', 'contextual ft_8008A2BC branches are absent from the runtime landing model'],
  ['ftCo_Squat_Anim', 'Squat', 'CrouchEnter', false, 'CrouchHold', 'ftCo_MS_SquatWait', 'ftCo_800D638C has item/context branches absent from the runtime model'],
  ['ftCo_SquatRv_Anim', 'SquatRv', 'CrouchExit', false, 'Idle', 'ftCo_MS_Wait', 'contextual ft_8008A2BC branches are absent from the runtime crouch-exit model'],
];

async function sourceText(path) {
  return readFile(resolve(sourceRoot, path), 'utf8');
}

function functionBody(text, symbol) {
  const start = text.indexOf(`void ${symbol}(`);
  if (start < 0) throw Error(`missing source callback ${symbol}`);
  const next = text.indexOf('\nvoid ', start + 6);
  return text.slice(start, next < 0 ? text.length : next);
}

export async function generate() {
  const source = JSON.parse(await readFile(sourcePath, 'utf8'));
  if (source.revision !== sourceRevision) throw Error(`source revision drift: ${source.revision}`);
  const callbacks = [];
  for (const [symbol, action, runtimePhase, forward, destination, sourceDestination, reason] of targetCallbacks) {
    const callback = source.callbacks.find(row => row.source.symbol === symbol);
    if (!callback) throw Error(`missing inventory callback ${symbol}`);
    if (callback.actions.length !== 1 && symbol !== 'ftCo_Landing_Anim') throw Error(`unexpected action set for ${symbol}`);
    const association = source.associations.find(row => row.callback === symbol && row.state === action);
    if (!association) throw Error(`missing inventory association ${symbol}/${action}`);
    const sourceState = source.states.find(row => row.name === action);
    if (!sourceState) throw Error(`missing inventory state ${action}`);
    if (callback.calls.length !== 2) throw Error(`${symbol} has ${callback.calls.length} calls`);
    const text = await sourceText(callback.source.path);
    const body = functionBody(text, symbol);
    for (const call of callback.calls) {
      if (!body.includes(call.symbol)) throw Error(`source body for ${symbol} omits ${call.symbol}`);
    }
    callbacks.push({
      source_callback_id: callback.id,
      source_association_id: association.id,
      source_state_id: sourceState.id,
      action,
      source: callback.source,
      runtime: {
        phase: runtimePhase,
        domain: 'ground',
        event: 'Motion',
        finished: true,
        forward,
        destination,
      },
      source_completion: {
        condition: '!ftAnim_IsFramesRemaining',
        destination: sourceDestination,
        qualification: 'unqualified',
        reason,
      },
      completion_call_id: callback.calls[0].id,
      ordered_calls: callback.calls.map(call => ({
        id: call.id,
        ordinal: call.ordinal,
        symbol: call.symbol,
        operation: call.operation,
        owner: call.owner,
        source: call.source,
      })),
    });
  }
  return {
    schema: 'games.animation-qualification.v1',
    profile: 'pigeon-falcon-gameplay',
    target: 'smash',
    source_inventory: 'classification/12_source_inventory.json',
    source_revision: sourceRevision,
    generator: 'classification/18_qualification.mjs',
    callbacks,
  };
}

export async function check() {
  const expected = await generate();
  const stored = JSON.parse(await readFile(outputPath, 'utf8'));
  if (JSON.stringify(stored) !== JSON.stringify(expected)) throw Error('stale qualification cases: run `node classification/18_qualification.mjs generate`');
  return expected;
}

export function runRuntimeQualification(base = resolve(here, '..'), env = process.env) {
  const stdout = execFileSync('cargo', [
    'run', '--locked', '--offline', '-j2', '--manifest-path', 'crates/fighter/Cargo.toml',
    '--example', '1_animation_qualification', '--quiet',
  ], {
    cwd: base,
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
    env: { ...env, CARGO_TARGET_DIR: env.CARGO_TARGET_DIR ?? '/private/tmp/games-qualification-target' },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  return JSON.parse(stdout);
}

export async function generateReceipts(base = resolve(here, '..')) {
  const cases = await generate();
  const raw = runRuntimeQualification(base);
  const fingerprint = currentSourceFingerprint(base);
  const runtimeRevision = currentRevision(base);
  const findings = raw.findings.map(receipt => ({
    ...receipt,
    sourceFingerprint: fingerprint,
    runtimeRevision,
    target: cases.target,
    sourceRevision: cases.source_revision,
  }));
  const output = {
    schema: raw.schema,
    status: raw.status,
    source_revision: cases.source_revision,
    source_fingerprint: fingerprint,
    runtime_revision: runtimeRevision,
    target: cases.target,
    generator: 'classification/18_qualification.mjs',
    findings,
    receipts: [],
  };
  await writeFile(receiptsPath, JSON.stringify(output, null, 2) + '\n');
  return output;
}

export async function checkReceipts(base = resolve(here, '..')) {
  const expected = await generateReceipts(base);
  const stored = JSON.parse(await readFile(receiptsPath, 'utf8'));
  if (JSON.stringify(stored) !== JSON.stringify(expected)) throw Error('stale qualification receipts: run `node classification/18_qualification.mjs receipts`');
  return expected;
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
  const mode = process.argv[2] ?? 'check';
  const run = mode === 'generate' ? generate() : mode === 'check' ? check() : mode === 'receipts' ? generateReceipts() : mode === 'receipts-check' ? checkReceipts() : Promise.reject(new Error('usage: 18_qualification.mjs generate|check|receipts|receipts-check'));
  run.then(value => console.log(JSON.stringify({ callbacks: value.callbacks?.length ?? value.findings?.length ?? 0, calls: value.callbacks?.flatMap(row => row.ordered_calls).length ?? 0, receipts: value.receipts?.length ?? 0 }))).catch(error => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
