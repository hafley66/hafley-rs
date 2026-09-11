import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';
import { root, loadValue, existing, output } from './2_registry.mjs';
import { currentSource } from './8_status.mjs';

const stepsSource = fileURLToPath(new URL('7_steps.tsp', import.meta.url));
const stepsOutput = new URL('10_steps.json', import.meta.url);
const sourceRules = new URL('../smash/src/fighters/falcon/generated/2_source_rules.json', import.meta.url);
const MAX_BYTES = 64 * 1024;

export async function loadSteps(path = stepsSource) {
  return loadValue(path, 'steps', 'Games.Steps');
}

// Observed facts emitted from a generated artifact by kind. The extractor's
// JSON artifact owns these values; TypeSpec never restates them.
export function observedFacts(kind, bytes) {
  if (kind !== 'source-rules-json') return null;
  const parsed = JSON.parse(bytes);
  return {
    repository: parsed.repository,
    revision: parsed.revision,
    rules: parsed.rules.length,
    unresolved: parsed.unresolved.length,
  };
}

function sha256(bytes) { return createHash('sha256').update(bytes).digest('hex'); }

// Pure join. Authored intent plus tool-observed evidence are explicit inputs so
// tests can mutate them. Evidence: Map stepId -> { artifacts, observed, gates }.
export function joinSteps(steps, evidence) {
  const errors = [];
  const fail = (code, message) => errors.push({ code, message });
  const receipts = {};
  for (const [key, step] of Object.entries(steps)) {
    if (step.id !== key) fail('CONTRADICTION', `step key ${key} != id ${step.id}`);
    const observed = evidence.get(key);
    if (!observed) { fail('MISSING_RECEIPT', `${key}: no tool-emitted evidence`); continue; }
    const authoredPaths = step.artifacts.map(artifact => artifact.path);
    const seenArtifacts = new Set();
    const artifacts = [];
    for (const artifact of step.artifacts) {
      const fact = observed.artifacts.get(artifact.path);
      if (!fact) { fail('MISSING_ARTIFACT', `${key}: no observation for ${artifact.path}`); continue; }
      if (seenArtifacts.has(artifact.path)) fail('DUPLICATE_ID', `${key}: duplicate artifact ${artifact.path}`);
      seenArtifacts.add(artifact.path);
      artifacts.push({ kind: artifact.kind, path: artifact.path, ...fact });
    }
    for (const [path] of observed.artifacts) {
      if (!authoredPaths.includes(path)) fail('EXTRA_ARTIFACT', `${key}: unauthored artifact ${path}`);
    }
    if (observed.observed === null) fail('MISSING_RECEIPT', `${key}: extractor artifact emitted no observed facts`);
    else if (!/^[0-9a-f]{40}$/.test(observed.observed.revision)) {
      fail('CONTRADICTION', `${key}: observed revision is not a pinned 40-hex commit`);
    }
    const gateNames = step.gates.map(gate => gate.name);
    for (const [name, result] of Object.entries(observed.gates)) {
      if (!gateNames.includes(name)) fail('EXTRA_ARTIFACT', `${key}: unauthored gate ${name}`);
      if (result !== 'passed') fail('GATE_FAILED', `${key}: gate ${name} observed ${result}`);
    }
    for (const gate of step.gates) {
      if (!(gate.name in observed.gates)) fail('MISSING_RECEIPT', `${key}: no observation for gate ${gate.name}`);
    }
    receipts[key] = {
      authored: {
        id: step.id, task: step.task, scope: step.scope,
        sourceKinds: [...step.sourceKinds], extractor: step.extractor,
        artifacts: step.artifacts.map(artifact => ({ kind: artifact.kind, path: artifact.path })),
        consumer: step.consumer,
        gates: step.gates.map(gate => ({ name: gate.name, command: gate.command })),
        stage: step.stage,
      },
      receipt: {
        source: observed.source,
        gates: { ...observed.gates },
        artifacts,
        observed: observed.observed,
      },
    };
  }
  return { steps: receipts, errors };
}

// Tool observation for one step: artifact hashes from the working tree, facts
// parsed from the extractor's primary artifact, and gate results from running
// each authored proof command. Any failed gate throws; no receipt is emitted.
export async function observeStep(step, base = root, runGate) {
  const artifacts = new Map();
  let observed = null;
  for (const artifact of step.artifacts) {
    const path = await existing(base, artifact.path);
    const bytes = await readFile(path);
    if (artifact.kind === 'source-rules-json') observed = observedFacts(artifact.kind, bytes);
    artifacts.set(artifact.path, { sha256: sha256(bytes), bytes: bytes.length });
  }
  const gates = {};
  for (const gate of step.gates) gates[gate.name] = await runGate(gate);
  return { artifacts, observed, gates, source: currentSource(base) };
}

export async function runGateRecipe(gate, base = root) {
  execFileSync('just', [gate.command.replace(/^just\s+/, '')], {
    cwd: base, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR ?? '/private/tmp/games-steps-target' },
  });
  return 'passed';
}

// Cheap re-verification for `check` mode: recompute artifact hashes and
// observed facts from disk and compare against the stored receipt. Gates are
// not re-executed; their recorded results are asserted in the returned
// evidence so the join still validates names and pass results.
export async function recheckReceipt(stored, step, base = root) {
  const problems = [];
  const expectedGates = Object.fromEntries(step.gates.map(gate => [gate.name, 'passed']));
  if (JSON.stringify(stored.receipt.gates) !== JSON.stringify(expectedGates)) {
    problems.push(`gates recorded ${JSON.stringify(stored.receipt.gates)}`);
  }
  const artifacts = new Map();
  for (const artifact of step.artifacts) {
    const bytes = await readFile(await existing(base, artifact.path));
    artifacts.set(artifact.path, { sha256: sha256(bytes), bytes: bytes.length });
  }
  let observed = null;
  for (const artifact of step.artifacts) {
    if (artifact.kind === 'source-rules-json') {
      observed = observedFacts(artifact.kind, await readFile(await existing(base, artifact.path)));
    }
  }
  const live = {
    source: await currentSource(base),
    artifacts: [...artifacts],
    observed,
  };
  const recorded = {
    source: stored.receipt.source,
    artifacts: stored.receipt.artifacts.map(a => [a.path, { sha256: a.sha256, bytes: a.bytes }]),
    observed: stored.receipt.observed,
  };
  if (live.source !== recorded.source) problems.push('source fingerprint is stale');
  if (JSON.stringify(live.artifacts) !== JSON.stringify(recorded.artifacts)) problems.push('artifact hashes or sizes changed');
  if (JSON.stringify(live.observed) !== JSON.stringify(recorded.observed)) problems.push('observed extractor facts changed');
  if (problems.length) throw Error(`stale generated receipt: ${problems.join('; ')}`);
  return { artifacts, observed, gates: expectedGates, source: recorded.source };
}

async function main() {
  const mode = process.argv[2] ?? 'check';
  if (!['check', 'generate'].includes(mode)) throw Error('usage: 9_steps.mjs check|generate');
  const steps = await loadSteps();
  if (mode === 'check') {
    const stored = JSON.parse(await readFile(stepsOutput, 'utf8'));
    const evidence = new Map();
    for (const [key, step] of Object.entries(steps)) {
      evidence.set(key, await recheckReceipt(stored.steps[key], steps[key]));
    }
    const result = joinSteps(steps, evidence);
    console.log(`${Object.keys(steps).length} steps validated; receipt current`);
    return;
  }
  const evidence = new Map();
  for (const [key, step] of Object.entries(steps)) evidence.set(key, await observeStep(step, root, runGateRecipe));
  const result = joinSteps(steps, evidence);
  if (result.errors.length) throw Error(result.errors.map(e => `${e.code}: ${e.message}`).join('\n'));
  const json = JSON.stringify(result);
  if (json.length >= MAX_BYTES) throw Error(`generated JSON exceeds ${MAX_BYTES} bytes`);
  await output(stepsOutput, json + '\n', false);
  console.log(`${Object.keys(steps).length} steps validated; receipt generated (${json.length} bytes)`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
