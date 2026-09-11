import { readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { root, loadRegistry, validateRegistry, renderD2, output, loadValue, existing } from './2_registry.mjs';
import { buildProgress, renderProgress, ingestRows } from './5_progress.mjs';
import { fingerprintSources } from '../shared/workflow/0_fingerprint.mjs';

// Required observation axes, authored in `7_status.tsp` and validated against
// this list. Each axis prints separately; no percentage combines them.
export const COLUMNS = ['payload', 'catalog', 'phase', 'chart', 'live', 'restore', 'native', 'fidelity'];

const statusSource = fileURLToPath(new URL('7_status.tsp', import.meta.url));
const statusProjection = new URL('11_status.json', import.meta.url);
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

// Pure join for an authored character (Pigeon). All inputs are explicit so
// tests can mutate them.
export function joinStatus({ status, axes, catalog, phases, phaseAnimation, ground, air, ingestRows: rows, mechanics, restore, native }) {
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
  const ingestByAction = new Map(rows.map(row => [row.action, row]));

  const joined = expected.map(entry => {
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

  return { rows: joined, errors };
}

// Pure join for a character whose generated catalog owns the ordered inventory
// (Dog). No action is authored in TypeSpec, so there is no family and no live
// phase model to observe; those axes stay explicit rather than guessed. Baked
// frame counts and manifest frame counts are cross-checked against the catalog,
// so a stale generated input fails the join.
export function joinDogStatus({ character, catalog, baked, ingestRows: rows, restore, native }) {
  const errors = [];
  const fail = (code, message) => errors.push({ code, message });

  if (JSON.stringify(character.axes) !== JSON.stringify(COLUMNS)) {
    fail('AXES', `authored axes ${JSON.stringify(character.axes)} do not match required ${JSON.stringify(COLUMNS)}`);
  }
  const entries = catalog?.entries ?? [];
  if (catalog?.runtime !== character.runtime) {
    fail('CONTRADICTION', `catalog runtime ${catalog?.runtime} != authored ${character.runtime}`);
  }
  if (catalog?.display_name !== character.display_name) {
    fail('CONTRADICTION', `catalog display name ${catalog?.display_name} != authored ${character.display_name}`);
  }

  const ids = new Set();
  const names = new Set();
  const files = new Set();
  entries.forEach((entry, index) => {
    if (entry.id !== index) fail('CATALOG_ORDER', `${character.runtime}: ${entry.name} has id ${entry.id}, expected ${index}`);
    if (ids.has(entry.id)) fail('DUPLICATE_ID', `${character.runtime}: duplicate catalog id ${entry.id}`);
    if (names.has(entry.name)) fail('DUPLICATE_ID', `${character.runtime}: duplicate catalog action ${entry.name}`);
    if (files.has(entry.file)) fail('DUPLICATE_ID', `${character.runtime}: duplicate catalog file ${entry.file}`);
    ids.add(entry.id);
    names.add(entry.name);
    files.add(entry.file);
  });

  if (character.baked) {
    if (!Array.isArray(baked)) {
      fail('MISSING_BAKED', `${character.runtime}: baked artifact is absent or unreadable`);
    } else if (baked.length !== entries.length) {
      fail('STALE_BAKED', `${character.runtime}: baked rows ${baked.length} != catalog ${entries.length}`);
    } else {
      entries.forEach((entry, index) => {
        const frames = baked[index]?.frames;
        if (!Array.isArray(frames) || frames.length !== entry.frames) {
          fail('STALE_BAKED', `${entry.name}: baked frames ${Array.isArray(frames) ? frames.length : '?'} != catalog ${entry.frames}`);
        }
      });
    }
  }

  const ingestByAction = new Map(rows.map(row => [row.action, row]));
  const joined = entries.map(entry => {
    const payload = ingestByAction.get(entry.name) ?? null;
    if (!payload) fail('MISSING_PAYLOAD', `${entry.name}: no retained ingest row`);
    else if (payload.state !== 'retained') fail('BROKEN_HASH', `${entry.name}: ${payload.file} ${payload.state}`);
    else if (Number.isInteger(payload.frames) && payload.frames !== entry.frames) {
      fail('STALE_FRAMES', `${entry.name}: manifest frames ${payload.frames} != catalog ${entry.frames}`);
    }
    return {
      id: entry.id,
      action: entry.name,
      family: null,
      payload,
      catalog: entry.file,
      phases: null,
      chart: null,
      live: null,
      restore,
      native,
      fidelity: 'UNKNOWN',
    };
  });

  return { rows: joined, errors };
}

// Checked generated artifacts only: the generated catalog owns inventory, the
// retained manifest owns payload bytes, the baked artifact must agree with the
// catalog. No Rust source is read or interpreted here.
export async function loadCharacterInputs(character, base = root) {
  const catalog = JSON.parse(await readFile(await existing(base, character.catalog), 'utf8'));
  const baked = character.baked
    ? JSON.parse(await readFile(await existing(base, character.baked), 'utf8'))
    : null;
  const { rows } = await ingestRows(base, { manifest: character.manifest });
  return { catalog, baked, rows };
}

// Pure projection. Only named records, no positional metadata, so the stale
// check compares exactly the tracked inputs' effect on the terminal matrix.
export function projectStatus(characters) {
  const projected = {};
  for (const [key, value] of Object.entries(characters)) {
    const record = {
      runtime: value.runtime,
      display_name: value.display_name,
      catalog: value.catalog,
      manifest: value.manifest,
      axes: [...value.axes],
      rows: value.rows,
    };
    if (value.baked) record.baked = value.baked;
    projected[key] = record;
  }
  return { characters: projected };
}

export function checkProjection(stored, current) {
  if (JSON.stringify(stored) !== JSON.stringify(current)) {
    const codes = Object.keys(current.characters).filter(key =>
      JSON.stringify(stored?.characters?.[key]) !== JSON.stringify(current.characters[key]));
    throw Error(`stale status projection: 11_status.json disagrees with tracked generated inputs (${codes.join(', ')}); run \`just status\``);
  }
  return current;
}

function renderMatrix(character, { rows, errors }) {
  const lines = [];
  lines.push(`character ${character.runtime} (${character.display_name}): ${rows.length} rows`);
  lines.push(`axes: ${character.axes.join(' | ')}  (independent; no percentage)`);
  lines.push('id  action        payload             catalog         phase                chart         live         restore   native    fidelity');
  for (const row of rows) {
    const payload = row.payload
      ? `${row.payload.state === 'retained' ? 'RET' : row.payload.state.toUpperCase()} ${(row.payload.hash ?? row.payload.expected).slice(0, 8)} f=${row.payload.frames ?? '?'}`
      : 'MISSING';
    const phase = row.phases === null ? 'UNIMPLEMENTED' : row.phases.length ? row.phases.join(',') : '-';
    const chart = row.chart === null ? 'UNIMPLEMENTED' : row.chart ? `y(${row.chart})` : '-';
    const live = row.live === null ? 'UNMEASURED' : row.live ? 'yes' : 'no';
    lines.push([
      String(row.id).padStart(2),
      row.action.padEnd(13),
      payload.padEnd(19),
      String(row.catalog).padEnd(15),
      phase.padEnd(20),
      chart.padEnd(13),
      live.padEnd(12),
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

// Live observation: authored characters plus the generated inputs they name.
// Pigeon still joins the executable Rust selection seam; Dog joins only
// committed generated artifacts.
export async function observeStatus(base = root) {
  const entries = await validateRegistry(await loadRegistry(), base);
  const progress = await buildProgress(base);
  const authored = await loadStatus();
  const current = currentSource(base);
  const prove = await readJson(resolve(workflow, 'prove.json'));
  const restore = receiptObservation(prove, current, ['core']);
  const native = receiptObservation(prove, current, ['export', 'browser']);
  const exported = runExport(base);
  const pigeonStatus = joinStatus({
    status: authored.characters.pigeon, axes: authored.characters.pigeon.axes,
    catalog: exported.catalog, phases: exported.phases,
    phaseAnimation: exported.phase_animation, ground: exported.ground, air: exported.air,
    ingestRows: progress.rows, mechanics: progress.mechanics,
    restore, native,
  });
  const dogCharacter = authored.characters.dog;
  const dogInputs = await loadCharacterInputs(dogCharacter, base);
  const dogStatus = joinDogStatus({
    character: dogCharacter, catalog: dogInputs.catalog, baked: dogInputs.baked,
    ingestRows: dogInputs.rows, restore, native,
  });
  const projection = projectStatus({
    pigeon: { ...authored.characters.pigeon, rows: pigeonStatus.rows },
    dog: { ...dogCharacter, rows: dogStatus.rows },
  });
  return {
    entries, progress, authored, current, prove, exported, projection,
    characters: {
      pigeon: { character: authored.characters.pigeon, ...pigeonStatus },
      dog: { character: dogCharacter, ...dogStatus },
    },
  };
}

async function main(mode = 'status') {
  if (!['status', 'check'].includes(mode)) throw Error('usage: 8_status.mjs [status|check]');
  const state = await observeStatus();
  await output(new URL('3_registry.json', import.meta.url), JSON.stringify(state.entries, null, 2) + '\n', true);
  await output(new URL('3_registry.d2', import.meta.url), renderD2(state.entries), true);
  await output(new URL('6_progress.html', import.meta.url), renderProgress(state.progress), true);

  if (mode === 'check') {
    const failures = [...state.characters.pigeon.errors, ...state.characters.dog.errors];
    if (failures.length) {
      throw Error(`status failures: ${failures.map(error => `${error.code} (${error.message})`).join('; ')}`);
    }
    const stored = await readJson(statusProjection);
    if (!stored) throw Error('stale status projection: 11_status.json is absent; run `just status`');
    checkProjection(stored, state.projection);
    console.log('status projection current: pigeon and dog matrices match tracked generated inputs');
    return;
  }

  await output(statusProjection, JSON.stringify(state.projection, null, 2) + '\n', false);

  const { exported, entries, characters, progress } = state;
  const extracted = await readJson(sourceRules);
  console.log('PACKAGES');
  for (const [name, entry] of Object.entries(entries)) {
    console.log(`  ${String(entry.stage).padEnd(4)} ${entry.destination.padEnd(9)} ${entry.task.padEnd(3)} ${name.padEnd(14)} ${entry.manifest ? 'manifest' : 'proposal'}`);
  }
  console.log('MATRIX');
  console.log(renderMatrix(characters.pigeon.character, characters.pigeon));
  console.log(renderMatrix(characters.dog.character, characters.dog));
  console.log('SOURCE RULES');
  console.log(`  ${extracted.rules.length} extracted from ${extracted.repository}@${extracted.revision.slice(0, 12)}`);
  for (const item of extracted.unresolved) console.log(`  UNRESOLVED ${item.symbol}: ${item.reason}`);
  const blocked = characters.pigeon.rows.filter(row => !row.live || row.chart === 0).length;
  console.log(`GATE game-fighter: stage ${entries['game-fighter'].stage} authored, not auto-promoted; ` +
    `${blocked}/${characters.pigeon.rows.length} Pigeon actions lack a live phase or executable chart mapping; ` +
    `${characters.pigeon.rows.filter(row => row.fidelity !== 'qualified').length} Pigeon actions unqualified on source fidelity; ` +
    `${characters.dog.rows.length} Dog actions from checked generated catalog, phase/chart/live unimplemented`);
  console.log(`source fingerprint: ${state.current ?? 'UNMEASURED, sibling runtime checkout absent'}; ` +
    `prove receipt: ${state.prove ? `${state.prove.commit} ${state.current === null ? 'UNVERIFIED' : state.prove.source === state.current ? 'current' : 'STALE'}` : 'absent'}`);
  const failures = [...characters.pigeon.errors, ...characters.dog.errors];
  if (failures.length) {
    console.error(`${failures.length} status failures`);
    process.exitCode = 1;
  }
}

export async function loadStatus() {
  return loadValue(statusSource, 'status', 'Games.Status');
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv[2] ?? 'status').catch(error => { console.error(error.message); process.exitCode = 1; });
}
