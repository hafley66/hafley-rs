import { readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { root, loadRegistry, validateRegistry, renderD2, output, loadValue, existing } from './2_registry.mjs';
import { buildProgress, renderProgress, loadProgress, ingestRows } from './5_progress.mjs';
import { fingerprintSources } from '../shared/workflow/0_fingerprint.mjs';

// Required observation axes, authored in `7_status.tsp` and validated against
// this list. Each axis prints separately; no percentage combines them.
export const COLUMNS = ['payload', 'catalog', 'phase', 'chart', 'live', 'restore', 'native', 'fidelity'];

async function fileDigest(base, path) {
  const bytes = await readFile(await existing(base, path));
  return { path, sha256: createHash('sha256').update(bytes).digest('hex'), bytes: bytes.length };
}

// Pure projection from checked generated inputs. Rows are the catalog's own
// membership and order; the generated role binding whose source is that action
// id is reported as `bindings`/`bound`, which proves binding only. The phase and
// live axes stay explicit UNKNOWN/unmeasured because no executable runtime
// consumer of the generated roles exists in this projection; chart stays
// explicit unknown because no generated per-character chart artifact exists.
// Independent axes, no percentage.
export async function buildStatusProjection(base = root) {
  const { characters: specs } = await loadStatus();
  const characters = [];
  for (const [key, spec] of Object.entries(specs)) {
    const catalog = JSON.parse(await readFile(await existing(base, spec.catalog), 'utf8'));
    const baked = JSON.parse(await readFile(await existing(base, spec.baked), 'utf8'));
    const roles = JSON.parse(await readFile(await existing(base, spec.roles), 'utf8'));
    const { rows: ingest } = await ingestRows(base, { manifest: spec.manifest });
    const ingestByAction = new Map(ingest.map(row => [row.action, row]));

    const errors = [];
    if (baked.length !== catalog.entries.length) {
      errors.push({
        code: 'BAKED_LENGTH',
        message: `${key}: ${baked.length} baked entries != ${catalog.entries.length} catalog actions`,
      });
    }

    const rolesBySource = new Map();
    for (const binding of roles.roles) {
      if (binding.source === null || binding.source === undefined) continue;
      if (!rolesBySource.has(binding.source)) rolesBySource.set(binding.source, []);
      rolesBySource.get(binding.source).push(binding.role);
    }

    const rows = catalog.entries.map((entry, index) => {
      if (entry.id !== index) {
        errors.push({ code: 'CATALOG_ORDER', message: `${key}: ${entry.name} id ${entry.id}, expected ${index}` });
      }
      const payload = ingestByAction.get(entry.name);
      if (!payload) errors.push({ code: 'MISSING_PAYLOAD', message: `${key}: ${entry.name} has no retained ingest row` });
      else if (payload.state !== 'retained') {
        errors.push({ code: 'BROKEN_HASH', message: `${key}: ${entry.name} ${payload.state}` });
      }
      const bakedFrames = baked[entry.id]?.frames?.length;
      if (bakedFrames !== entry.frames) {
        errors.push({
          code: 'BAKED_FRAMES',
          message: `${key}: ${entry.name} baked ${bakedFrames} frames != catalog ${entry.frames}`,
        });
      }
      const bindings = (rolesBySource.get(entry.id) ?? []).slice().sort();
      return {
        id: entry.id,
        action: entry.name,
        file: entry.file,
        frames: entry.frames,
        payload: {
          state: payload?.state ?? 'missing',
          hash: payload?.hash ?? null,
          expected: payload?.expected ?? null,
          frames: payload?.frames ?? null,
        },
        bindings,
        bound: bindings.length > 0,
        phase: 'UNKNOWN',
        live: false,
      };
    });

    for (const sourceId of rolesBySource.keys()) {
      if (!catalog.entries.some(entry => entry.id === sourceId)) {
        errors.push({
          code: 'IMPOSSIBLE_MAPPING',
          message: `${key}: role binding references unknown action id ${sourceId}`,
        });
      }
    }

    const catalogDigest = await fileDigest(base, spec.catalog);
    const bakedDigest = await fileDigest(base, spec.baked);
    characters.push({
      key,
      display: spec.display,
      catalog: { ...catalogDigest, count: catalog.entries.length },
      baked: { ...bakedDigest, count: baked.length },
      roles: await fileDigest(base, spec.roles),
      manifest: await fileDigest(base, spec.manifest),
      rows,
      errors,
    });
  }
  return { characters };
}

export function renderCharacterMatrix(character) {
  const lines = [];
  lines.push(`status axes: ${COLUMNS.join(' | ')}  (independent; no percentage)`);
  lines.push('id  action        payload             catalog         bind                 phase      chart  live  restore   native    fidelity');
  for (const row of character.rows) {
    const payload = row.payload.hash
      ? `${row.payload.state === 'retained' ? 'RET' : row.payload.state.toUpperCase()} ${(row.payload.hash ?? row.payload.expected).slice(0, 8)} f=${row.payload.frames ?? row.frames ?? '?'}`
      : 'MISSING';
    const bindings = row.bindings.length ? row.bindings.join(',') : '-';
    lines.push([
      String(row.id).padStart(2),
      row.action.padEnd(13),
      payload.padEnd(19),
      String(row.file).padEnd(15),
      bindings.padEnd(20),
      row.phase.padEnd(10),
      '-'.padEnd(6),
      (row.live ? 'yes' : 'no').padEnd(5),
      'UNMEASURED'.padEnd(9),
      'UNMEASURED'.padEnd(9),
      'UNKNOWN',
    ].join(' '));
  }
  if (character.errors.length) {
    lines.push(`FAILURES (${character.errors.length}):`);
    for (const error of character.errors) lines.push(`  ${error.code}: ${error.message}`);
  } else {
    lines.push('FAILURES: none');
  }
  return lines.join('\n');
}

const projectionOutput = new URL('11_status.json', import.meta.url);

export async function checkStatusProjection(live) {
  if (!live) live = await buildStatusProjection();
  const stored = JSON.parse(await readFile(projectionOutput, 'utf8'));
  if (JSON.stringify(stored) !== JSON.stringify(live)) {
    throw Error('stale status projection: generated input or committed projection changed; run `node classification/8_status.mjs generate`');
  }
  return live;
}

export async function writeStatusProjection(base = root) {
  const projection = await buildStatusProjection(base);
  await output(projectionOutput, JSON.stringify(projection, null, 2) + '\n', false);
  return projection;
}

const statusSource = fileURLToPath(new URL('7_status.tsp', import.meta.url));
const workflow = fileURLToPath(new URL('../blender-godot-sqlite-proof/pigeon-lab/.workflow/', import.meta.url));
const sourceRules = fileURLToPath(new URL('../smash/src/fighters/pigeon/generated/2_source_rules.json', import.meta.url));

// The live Rust export writes JSON to stdout. No checked editable registry.
export function runExport(base = root, env = process.env) {
  const stdout = execFileSync('cargo', [
    'run', '--locked', '--offline', '-j2', '--manifest-path', 'smash/Cargo.toml',
    '--example', 'status_export', '--quiet',
  ], {
    cwd: base, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...env, CARGO_TARGET_DIR: env.CARGO_TARGET_DIR ?? '/private/tmp/games-status-target' },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  return JSON.parse(stdout);
}

// Mirrors the roots in pigeon-lab/100_workflow.mjs so `receipt.source` is
// comparable. Returns null when the sibling runtime checkout is absent, in
// which case receipt freshness is UNMEASURED rather than assumed.
export function currentSource(base = root) {
  const repo = execFileSync('git', ['rev-parse', '--show-toplevel'], { cwd: base, encoding: 'utf8' }).trim();
  const sibling = resolve(repo, '../hafley-rs-game-runtime');
  if (!existsSync(sibling)) return null;
  return fingerprintSources([
    [repo, ['.gitmodules', 'AGENTS.md', 'games/AGENTS.md', 'games/crates', 'games/shared',
      'games/smash', 'games/blender-godot-sqlite-proof']],
    [sibling, ['tools/godot-web', 'games/kneeman/app/deploy/scripts']],
  ]);
}

async function readJson(path) {
  try { return JSON.parse(await readFile(path, 'utf8')); } catch (error) {
    if (error.code === 'ENOENT') return null;
    throw error;
  }
}

function stagesPassed(receipt, names) {
  return names.every(name => receipt?.stages?.some(stage => stage.name === name && stage.status === 'passed'));
}

// A receipt observation is only a pass when its recorded source fingerprint
// equals the recomputed one. A listed path is never a passing test.
export function receiptObservation(receipt, current, names) {
  if (!receipt) return 'UNMEASURED';
  if (current === null) return 'UNVERIFIED, source not recomputable';
  if (receipt.source !== current) return 'STALE';
  if (receipt.status !== 'passed') return 'FAILED';
  return stagesPassed(receipt, names) ? 'PASSED' : 'FAILED';
}

function familyFidelity(mechanics) {
  const families = new Map();
  for (const [key, mechanic] of Object.entries(mechanics)) {
    if (!families.has(mechanic.family)) families.set(mechanic.family, new Map());
    families.get(mechanic.family).set(key, mechanic.fidelity);
  }
  return families;
}

// Pure join. All inputs are explicit so tests can mutate them.
export function joinStatus({ status, axes, catalog, phases, phaseAnimation, ground, air, ingestRows, mechanics, restore, native }) {
  const errors = [];
  const fail = (code, message) => errors.push({ code, message });

  if (JSON.stringify(axes) !== JSON.stringify(COLUMNS)) {
    fail('AXES', `authored axes ${JSON.stringify(axes)} do not match required ${JSON.stringify(COLUMNS)}`);
  }
  if (new Set(axes).size !== axes.length) fail('DUPLICATE_ID', 'authored axes contain duplicates');

  const ids = new Set();
  const names = new Set();
  for (const [key, entry] of Object.entries(status.expected)) {
    if (entry.action !== key) fail('CONTRADICTION', `expected key ${key} != action ${entry.action}`);
    if (ids.has(entry.id)) fail('DUPLICATE_ID', `duplicate expected id ${entry.id}`);
    if (names.has(entry.action)) fail('DUPLICATE_ID', `duplicate expected action ${entry.action}`);
    ids.add(entry.id);
    names.add(entry.action);
  }
  const expected = Object.entries(status.expected)
    .map(([key, entry]) => ({ ...entry, key }))
    .sort((a, b) => a.id - b.id);
  expected.forEach((entry, index) => {
    if (entry.id !== index) fail('MISSING_STABLE_ID', `expected ids are not contiguous at ${entry.action}=${entry.id}`);
  });

  const catalogById = new Map(catalog.map(entry => [entry.id, entry]));
  const catalogIds = new Set(catalog.map(entry => entry.id));
  if (catalogIds.size !== catalog.length) fail('DUPLICATE_ID', 'export catalog contains duplicate ids');
  catalog.forEach((entry, index) => {
    if (entry.id !== index) fail('CATALOG_ORDER', `catalog entry ${entry.action} has id ${entry.id}, expected ${index}`);
  });
  for (const entry of expected) {
    const observed = catalogById.get(entry.id);
    if (!observed) { fail('MISSING_STABLE_ID', `catalog has no id ${entry.id} (${entry.action})`); continue; }
    if (observed.action !== entry.action) {
      fail('CONTRADICTION', `id ${entry.id}: expected ${entry.action}, export has ${observed.action}`);
    }
  }
  for (const entry of catalog) {
    if (!expected.some(e => e.id === entry.id)) fail('EXTRA_CATALOG', `export catalog has unowned id ${entry.id} ${entry.action}`);
  }

  const phaseNames = new Set(phases.map(phase => phase.name));
  if (phaseNames.size !== phases.length) fail('DUPLICATE_ID', 'export phases contain duplicates');

  const actionPhases = new Map();
  for (const entry of phaseAnimation) {
    if (!catalogIds.has(entry.action)) {
      fail('IMPOSSIBLE_MAPPING', `phase animation references unknown action id ${entry.action}`);
      continue;
    }
    if (!phaseNames.has(entry.phase)) {
      fail('IMPOSSIBLE_MAPPING', `phase animation references unknown phase ${entry.phase}`);
      continue;
    }
    if (!actionPhases.has(entry.action)) actionPhases.set(entry.action, new Set());
    actionPhases.get(entry.action).add(entry.phase);
  }

  const transitions = [...ground.map(t => ({ ...t, kind: 'ground' })), ...air.map(t => ({ ...t, kind: 'air' }))];
  for (const transition of transitions) {
    for (const phase of [transition.from, transition.to].filter(Boolean)) {
      if (!phaseNames.has(phase)) {
        fail('IMPOSSIBLE_MAPPING', `${transition.kind} transition references unknown phase ${phase}`);
      }
    }
  }

  const families = familyFidelity(mechanics);
  const ingestByAction = new Map(ingestRows.map(row => [row.action, row]));

  const rows = expected.map(entry => {
    const observed = catalogById.get(entry.id);
    const payload = ingestByAction.get(entry.action);
    if (!payload) fail('MISSING_PAYLOAD', `${entry.action}: no retained ingest row`);
    else if (payload.state !== 'retained') {
      fail('BROKEN_HASH', `${entry.action}: ${payload.file} ${payload.state}`);
    }

    const mapped = [...(actionPhases.get(entry.id) ?? [])].sort();
    const chart = transitions.filter(t => mapped.includes(t.from) || mapped.includes(t.to));
    const familyHits = families.get(entry.family);
    if (!familyHits) {
      fail('MISSING_STABLE_ID', `${entry.action}: unknown mechanic family ${entry.family}`);
    } else if (new Set(familyHits.values()).size !== 1) {
      fail('CONTRADICTION', `${entry.action}: family ${entry.family} has conflicting fidelity dispositions`);
    }
    return {
      id: entry.id,
      action: entry.action,
      family: entry.family,
      payload,
      catalog: observed?.file ?? null,
      phases: mapped,
      chart: chart.length,
      live: mapped.length > 0,
      restore,
      native,
      fidelity: familyHits ? [...familyHits.values()][0] : 'UNKNOWN',
    };
  });

  return { rows, errors };
}

function renderMatrix({ rows, errors }) {
  const lines = [];
  lines.push(`status axes: ${COLUMNS.join(' | ')}  (independent; no percentage)`);
  lines.push('id  action        payload             catalog         phase                chart  live  restore   native    fidelity');
  for (const row of rows) {
    const payload = row.payload
      ? `${row.payload.state === 'retained' ? 'RET' : row.payload.state.toUpperCase()} ${(row.payload.hash ?? row.payload.expected).slice(0, 8)} f=${row.payload.frames ?? '?'}`
      : 'MISSING';
    const phase = row.phases.length ? row.phases.join(',') : '-';
    lines.push([
      String(row.id).padStart(2),
      row.action.padEnd(13),
      payload.padEnd(19),
      String(row.catalog).padEnd(15),
      phase.padEnd(20),
      row.chart ? `y(${row.chart})`.padEnd(6) : '-'.padEnd(6),
      (row.live ? 'yes' : 'no').padEnd(5),
      row.restore.padEnd(9),
      row.native.padEnd(9),
      row.fidelity,
    ].join(' '));
  }
  if (errors.length) {
    lines.push(`FAILURES (${errors.length}):`);
    for (const error of errors) lines.push(`  ${error.code}: ${error.message}`);
  } else {
    lines.push('FAILURES: none');
  }
  return lines.join('\n');
}

async function main() {
  const mode = process.argv[2] ?? 'check';
  if (!['check', 'generate'].includes(mode)) throw Error('usage: 8_status.mjs check|generate');
  const entries = await validateRegistry(await loadRegistry());
  await output(new URL('3_registry.json', import.meta.url), JSON.stringify(entries, null, 2) + '\n', true);
  await output(new URL('3_registry.d2', import.meta.url), renderD2(entries), true);
  const progress = await buildProgress();
  await output(new URL('6_progress.html', import.meta.url), renderProgress(progress), true);
  const projection = mode === 'generate' ? await writeStatusProjection() : await checkStatusProjection();
  const authored = await loadStatus();
  const exported = runExport();
  const extracted = await readJson(sourceRules);
  const current = currentSource();
  const prove = await readJson(resolve(workflow, 'prove.json'));
  const result = joinStatus({
    status: authored, axes: authored.axes,
    catalog: exported.catalog, phases: exported.phases,
    phaseAnimation: exported.phase_animation, ground: exported.ground, air: exported.air,
    ingestRows: progress.rows, mechanics: progress.mechanics,
    restore: receiptObservation(prove, current, ['core']),
    native: receiptObservation(prove, current, ['export', 'browser']),
  });
  console.log('PACKAGES');
  for (const [name, entry] of Object.entries(entries)) {
    console.log(`  ${String(entry.stage).padEnd(4)} ${entry.destination.padEnd(9)} ${entry.task.padEnd(3)} ${name.padEnd(14)} ${entry.manifest ? 'manifest' : 'proposal'}`);
  }
  console.log('MATRIX PIGEON');
  console.log(renderMatrix(result));
  const dog = projection.characters.find(character => character.key === 'dog');
  console.log('MATRIX DOG');
  console.log(renderCharacterMatrix(dog));
  console.log('SOURCE RULES');
  console.log(`  ${extracted.rules.length} extracted from ${extracted.repository}@${extracted.revision.slice(0, 12)}`);
  for (const item of extracted.unresolved) console.log(`  UNRESOLVED ${item.symbol}: ${item.reason}`);
  console.log(`STATUS PROJECTION: ${projection.characters.map(character =>
    `${character.key} ${character.rows.length} actions ${mode === 'generate' ? 'generated' : 'current'}`).join('; ')}`);
  const blocked = result.rows.filter(row => !row.live || row.chart === 0).length;
  console.log(`GATE game-fighter: stage ${entries['game-fighter'].stage} authored, not auto-promoted; ` +
    `${blocked}/${result.rows.length} Pigeon actions lack a live phase or executable chart mapping; ` +
    `${result.rows.filter(row => row.fidelity !== 'qualified').length} actions unqualified on source fidelity`);
  console.log(`source fingerprint: ${current ?? 'UNMEASURED, sibling runtime checkout absent'}; ` +
    `prove receipt: ${prove ? `${prove.commit} ${current === null ? 'UNVERIFIED' : prove.source === current ? 'current' : 'STALE'}` : 'absent'}`);
  const projectionErrors = projection.characters.flatMap(character => character.errors.map(error => `${character.key} ${error.code}: ${error.message}`));
  if (projectionErrors.length) {
    console.error(`${projectionErrors.length} projection failures`);
    process.exitCode = 1;
  }
  if (result.errors.length) {
    console.error(`${result.errors.length} status failures`);
    process.exitCode = 1;
  }
  if (dog && dog.rows.length !== 25) {
    console.error(`dog projection has ${dog.rows.length} rows, expected 25`);
    process.exitCode = 1;
  }
}

export async function loadStatus() {
  return loadValue(statusSource, 'status', 'Games.Status');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
