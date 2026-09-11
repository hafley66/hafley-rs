import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fingerprintSources } from '../shared/workflow/0_fingerprint.mjs';
import { loadValue, output, root } from './2_registry.mjs';

export const AXES = [
  'state mapping',
  'transition/callback mapping',
  'ordered guard qualification',
  'live reachability',
  'rollback',
  'source fidelity',
];

const portSource = fileURLToPath(new URL('13_port.tsp', import.meta.url));
const sourcePath = fileURLToPath(new URL('12_source_inventory.json', import.meta.url));
const runtimePath = fileURLToPath(new URL('14_runtime_inventory.json', import.meta.url));
const coveragePath = fileURLToPath(new URL('16_coverage.json', import.meta.url));
const textPath = fileURLToPath(new URL('17_coverage.txt', import.meta.url));
const provePath = fileURLToPath(new URL('../blender-godot-sqlite-proof/pigeon-lab/.workflow/prove.json', import.meta.url));

export async function loadPort() {
  return loadValue(portSource, 'port', 'Games.Port');
}

async function json(path) {
  return JSON.parse(await readFile(path, 'utf8'));
}

export function currentRevision(base = root) {
  return execFileSync('git', ['rev-parse', 'HEAD'], { cwd: base, encoding: 'utf8' }).trim();
}

export function currentSourceFingerprint(base = root) {
  const repo = execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd: base, encoding: 'utf8' }).trim();
  const sibling = resolve(repo, '../hafley-rs-game-runtime');
  const roots = [[repo, [
    '.gitmodules', 'AGENTS.md', 'games/AGENTS.md', 'games/crates', 'games/shared',
    'games/smash', 'games/blender-godot-sqlite-proof',
  ]]];
  return fingerprintSources(roots.concat([[sibling, ['tools/godot-web', 'games/kneeman/app/deploy/scripts']]]));
}

export function runRuntimeExport(base = root, env = process.env) {
  const stdout = execFileSync('cargo', [
    'run', '--locked', '--offline', '-j2', '--manifest-path', 'smash/Cargo.toml',
    '--example', 'status_export', '--quiet',
  ], {
    cwd: base,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    env: { ...env, CARGO_TARGET_DIR: env.CARGO_TARGET_DIR ?? '/private/tmp/games-status-target' },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  return JSON.parse(stdout);
}

export function runtimeObservation(raw, port, source, base = root) {
  return {
    profile: port.profile.id,
    source_revision: source.revision,
    runtime_revision: currentRevision(base),
    source_fingerprint: currentSourceFingerprint(base),
    runtime: raw.runtime_inventory,
    // The full selector cube is a capability; live bindings are the actions
    // reached by the deterministic runtime tape.
    pigeon_bindings: raw.pigeon_live_bindings ?? raw.phase_animation,
    pigeon_selection_capability: raw.phase_animation,
    ground: raw.ground,
    air: raw.air,
  };
}

export function expandExclusions(states, rules) {
  const rows = [];
  const counts = Object.fromEntries(rules.map(rule => [rule.id, 0]));
  for (const state of states) {
    const rule = rules.find(candidate => candidate.namePrefixes.some(prefix => state.name.startsWith(prefix)));
    if (!rule) continue;
    counts[rule.id] += 1;
    rows.push({ source_id: state.id, name: state.name, rule: rule.id, reason: rule.reason, evidence: rule.evidence });
  }
  return { rows, counts };
}

function ratio(numerator, denominator) {
  return {
    numerator,
    denominator,
    percentage: denominator === 0 ? null : Number((numerator * 100 / denominator).toFixed(2)),
  };
}

function receiptState(receipt, fingerprint, stage) {
  if (!receipt) return 'UNMEASURED';
  if (receipt.source !== fingerprint) return 'STALE';
  if (receipt.status !== 'passed') return 'FAILED';
  return receipt.stages?.some(item => item.name === stage && item.status === 'passed') ? 'PASSED' : 'FAILED';
}

function reachable(runtime) {
  if (Array.isArray(runtime.reachable_states)) return new Set(runtime.reachable_states);
  const edges = [...(runtime.ground ?? []), ...(runtime.air ?? [])];
  const seen = new Set(['Idle']);
  let changed = true;
  while (changed) {
    changed = false;
    for (const edge of edges) {
      if (seen.has(edge.from) && edge.to && !seen.has(edge.to)) {
        seen.add(edge.to);
        changed = true;
      }
    }
  }
  return seen;
}

function callbackAssociations(source) {
  if (Array.isArray(source.associations)) return source.associations;
  return (source.callbacks ?? []).flatMap(row => {
    const actions = row.actions ?? (row.action === undefined ? [null] : [row.action]);
    return actions.map(action => ({ ...row, action }));
  });
}

function directCallRows(source) {
  if (Array.isArray(source.direct_calls)) return source.direct_calls;
  return (source.callbacks ?? []).flatMap(row => row.calls ?? []);
}

function runtimeCallbackNames(runtime) {
  return new Set((runtime.runtime?.callbacks ?? []).map(callback =>
    typeof callback === 'string'
      ? callback
      : callback.id ?? callback.name ?? callback.symbol ?? `${callback.domain}:${callback.state}:${callback.event}`));
}

// Join authored intent with source and runtime observations. No authored
// disposition contributes to an observed numerator.
export function joinCoverage({ port, source, runtime, receipt, fingerprint, runtimeRevision }) {
  const errors = [];
  const sourceById = new Map(source.states.map(row => [row.id, row]));
  const runtimeStates = new Set((runtime.runtime?.states ?? []).map(row => row.name));
  const runtimeCallbacks = runtimeCallbackNames(runtime);
  const exclusions = expandExclusions(source.states, port.exclusions);
  const excludedIds = new Set(exclusions.rows.map(row => row.source_id));
  const mappingBySource = new Map();
  for (const mapping of port.mappings) {
    if (!sourceById.has(mapping.sourceId)) errors.push(`mapping references unknown source id ${mapping.sourceId}`);
    if (!runtimeStates.has(mapping.runtimeState)) errors.push(`mapping references unknown runtime state ${mapping.runtimeState}`);
    if (mappingBySource.has(mapping.sourceId)) errors.push(`duplicate mapping for source id ${mapping.sourceId}`);
    mappingBySource.set(mapping.sourceId, mapping);
  }
  const callbackRows = callbackAssociations(source);
  const callbackBySource = new Map();
  for (const mapping of port.callbackMappings ?? []) {
    if (!callbackRows.some(row => row.id === mapping.sourceId)) errors.push(`callback mapping references unknown source id ${mapping.sourceId}`);
    if (!runtimeCallbacks.has(mapping.runtimeCallback)) errors.push(`callback mapping references unknown runtime callback ${mapping.runtimeCallback}`);
    callbackBySource.set(mapping.sourceId, mapping);
  }
  const guardRows = directCallRows(source);
  const guardIds = new Set(guardRows.map(row => row.id));
  const guardBySource = new Map();
  for (const mapping of port.guardMappings ?? []) {
    if (!guardIds.has(mapping.sourceId)) errors.push(`guard mapping references unknown source id ${mapping.sourceId}`);
    guardBySource.set(mapping.sourceId, mapping);
  }
  for (const row of exclusions.rows) {
    if (mappingBySource.has(row.source_id)) errors.push(`source row both mapped and excluded: ${row.source_id}`);
  }
  for (const rule of port.exclusions) {
    if (exclusions.counts[rule.id] === 0) errors.push(`exclusion rule matched no source rows: ${rule.id}`);
  }
  if (port.profile.sourceRevision !== source.revision) errors.push('profile source revision does not match source inventory');
  if (runtime.profile !== port.profile.id) errors.push('runtime inventory profile mismatch');
  if (runtime.source_revision !== source.revision) errors.push('runtime inventory source revision mismatch');
  if (runtimeRevision && runtime.runtime_revision !== runtimeRevision) errors.push('runtime inventory revision is stale');

  const eligible = source.states.filter(row => !excludedIds.has(row.id));
  const mapped = eligible.filter(row => mappingBySource.has(row.id) && runtimeStates.has(mappingBySource.get(row.id).runtimeState));
  const graphReachableStates = reachable(runtime.runtime ?? {});
  const liveRows = runtime.pigeon_bindings ?? [];
  const livePhases = liveRows.length ? new Set(liveRows.map(row => row.phase)) : null;
  const reachableStates = livePhases
    ? new Set([...graphReachableStates].filter(state => livePhases.has(state)))
    : graphReachableStates;
  const live = mapped.filter(row => reachableStates.has(mappingBySource.get(row.id).runtimeState));
  const callbacks = callbackRows;
  const directCalls = guardRows;
  const stale = receiptState(receipt, fingerprint, 'core') !== 'PASSED';
  const currentReceipt = receiptState(receipt, fingerprint, 'core');
  const mappedCallbacks = callbackRows.filter(row => callbackBySource.has(row.id));
  const mappedGuards = guardRows.filter(row => guardBySource.has(row.id));
  const directCallCount = source.counts?.direct_calls ?? directCalls.length;
  const axes = {
    'state mapping': ratio(mapped.length, eligible.length),
    'transition/callback mapping': ratio(mappedCallbacks.length, callbacks.length),
    'ordered guard qualification': ratio(mappedGuards.length, directCallCount),
    'live reachability': ratio(live.length, eligible.length),
    'rollback': ratio(stale ? 0 : mapped.length, eligible.length),
    'source fidelity': ratio(stale ? 0 : mapped.length, eligible.length),
  };
  const fullyQualified = mapped.filter(row => !stale && reachableStates.has(mappingBySource.get(row.id).runtimeState));
  const unresolvedStates = eligible.filter(row => !mappingBySource.has(row.id)).map(row => ({ source_id: row.id, name: row.name, reason: 'no authored source-to-runtime mapping' }));
  const unresolvedCallbacks = callbacks.filter(row => !callbackBySource.has(row.id)).map(row => ({ source_id: row.id, action: row.action, symbol: row.source.symbol, reason: 'no authored callback mapping' }));
  const unresolvedDirectCalls = directCalls.filter(row => !guardBySource.has(row.id)).map(row => ({ source_id: row.id, symbol: row.symbol, reason: 'no authored ordered guard qualification' }));
  return {
    profile: port.profile,
    source: { ruleset: source.ruleset, repository: source.repository, revision: source.revision, counts: source.counts },
    runtime: { profile: runtime.profile, revision: runtime.runtime_revision, source_revision: runtime.source_revision, source_fingerprint: runtime.source_fingerprint, states: runtime.runtime.states.length },
    receipts: { rollback: currentReceipt, source_fidelity: currentReceipt, fingerprint },
    exclusions: { rules: exclusions.counts, rows: exclusions.rows },
    axes,
    strict: ratio(fullyQualified.length, eligible.length),
    unresolved: { states: unresolvedStates, callbacks: unresolvedCallbacks, direct_calls: unresolvedDirectCalls },
    errors,
  };
}

export function renderCoverage(coverage) {
  const lines = [
    `profile: ${coverage.profile.id}`,
    `source revision: ${coverage.source.revision}`,
    `runtime revision: ${coverage.runtime.revision}`,
    `source fingerprint: ${coverage.receipts.fingerprint ?? 'UNMEASURED'}`,
    `exclusions: ${coverage.exclusions.rows.length} source states (${Object.entries(coverage.exclusions.rules).map(([id, count]) => `${id}=${count}`).join(', ')})`,
  ];
  for (const axis of coverage.profile.axes) {
    const value = coverage.axes[axis];
    lines.push(`${axis}: ${value.numerator}/${value.denominator} (${value.percentage === null ? 'UNMEASURED' : `${value.percentage}%`})`);
  }
  lines.push(`strict fully-qualified intersection: ${coverage.strict.numerator}/${coverage.strict.denominator} (${coverage.strict.percentage}%)`);
  lines.push(`unresolved: states=${coverage.unresolved.states.length} callbacks=${coverage.unresolved.callbacks.length} direct_calls=${coverage.unresolved.direct_calls.length}`);
  if (coverage.errors.length) lines.push(`errors: ${coverage.errors.join('; ')}`);
  return lines.join('\n') + '\n';
}

export async function buildCoverage(base = root, { raw = undefined, runtime = undefined, source = undefined, port = undefined, receipt = undefined, fingerprint = undefined } = {}) {
  source ??= await json(sourcePath);
  port ??= await loadPort();
  raw ??= runRuntimeExport(base);
  runtime ??= runtimeObservation(raw, port, source, base);
  receipt ??= await json(provePath).catch(error => error.code === 'ENOENT' ? null : Promise.reject(error));
  fingerprint ??= currentSourceFingerprint(base);
  return joinCoverage({ port, source, runtime, receipt, fingerprint, runtimeRevision: currentRevision(base) });
}

export async function generate(base = root) {
  const source = await json(sourcePath);
  const port = await loadPort();
  const raw = runRuntimeExport(base);
  const runtime = runtimeObservation(raw, port, source, base);
  await output(runtimePath, JSON.stringify(runtime, null, 2) + '\n', false);
  const coverage = await buildCoverage(base, { raw, source, port, runtime });
  await output(coveragePath, JSON.stringify(coverage, null, 2) + '\n', false);
  await output(textPath, renderCoverage(coverage), false);
  return coverage;
}

export async function check(base = root) {
  const expected = await buildCoverage(base);
  const stored = await json(coveragePath);
  if (JSON.stringify(stored) !== JSON.stringify(expected)) throw Error('stale coverage output: run `node classification/15_coverage.mjs generate`');
  if (await readFile(textPath, 'utf8') !== renderCoverage(expected)) throw Error('stale coverage text: run `node classification/15_coverage.mjs generate`');
  const raw = runRuntimeExport(base);
  const port = await loadPort();
  const source = await json(sourcePath);
  const current = runtimeObservation(raw, port, source, base);
  const storedRuntime = await json(runtimePath);
  if (JSON.stringify(storedRuntime) !== JSON.stringify(current)) throw Error('stale runtime inventory: run `node classification/15_coverage.mjs generate`');
  if (expected.errors.length) throw Error(expected.errors.join('; '));
  return expected;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const mode = process.argv[2] ?? 'check';
  const action = mode === 'generate' ? generate() : mode === 'check' ? check() : Promise.reject(new Error('usage: 15_coverage.mjs generate|check'));
  action.then(result => console.log(JSON.stringify(result.axes))).catch(error => { console.error(error.message); process.exitCode = 1; });
}
