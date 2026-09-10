import { readFile, stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

// Self-contained: no TypeSpec compiler import. Membership/order come from the
// executable export, retained facts from the fighter manifest.
export const root = fileURLToPath(new URL('../', import.meta.url));

// Retained ingest manifest: filenames, declared frame counts and expected SHA256.
export const MANIFEST = 'smash/src/fighters/falcon/imported/0_sources.json';
const IMPORTED = 'smash/src/fighters/falcon/imported';

// The live Rust export writes JSON to stdout. Membership, order and the
// Phase-to-action selection are executable facts; nothing is re-authored here.
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

export async function loadManifest(base = root) {
  return JSON.parse(await readFile(resolve(base, MANIFEST), 'utf8'));
}

// Recompute presence and SHA256 for every retained filename. A missing file is
// null, never a pass.
export async function fileHashes(files, base = root) {
  const hashes = {};
  for (const file of files) {
    const path = resolve(base, IMPORTED, file);
    const scoped = relative(resolve(base), path);
    if (!scoped || scoped === '..' || scoped.startsWith('../')) { hashes[file] = null; continue; }
    try {
      if (!(await stat(path)).isFile()) { hashes[file] = null; continue; }
      hashes[file] = createHash('sha256').update(await readFile(path)).digest('hex');
    } catch {
      hashes[file] = null;
    }
  }
  return hashes;
}

// Pure join. Every input is explicit so tests can mutate it.
export function joinStatus({ catalog, phases, phaseAnimation, manifestFiles, manifestFrames, hashes }) {
  const errors = [];
  const fail = (code, message) => errors.push({ code, message });

  const ids = new Set();
  const names = new Set();
  const files = new Set();
  catalog.forEach((entry, index) => {
    if (entry.id !== index) fail('CATALOG_ORDER', `catalog ${entry.action} has id ${entry.id}, expected ${index}`);
    if (ids.has(entry.id)) fail('DUPLICATE_ID', `duplicate catalog id ${entry.id}`);
    if (names.has(entry.action)) fail('DUPLICATE_NAME', `duplicate catalog action ${entry.action}`);
    if (files.has(entry.file)) fail('DUPLICATE_FILE', `duplicate catalog file ${entry.file}`);
    ids.add(entry.id); names.add(entry.action); files.add(entry.file);
  });

  const phaseNames = new Set();
  for (const phase of phases) {
    if (phaseNames.has(phase.name)) fail('DUPLICATE_ID', `duplicate phase ${phase.name}`);
    phaseNames.add(phase.name);
  }

  const actionPhases = new Map();
  const mappedPhases = new Set();
  for (const entry of phaseAnimation) {
    if (!phaseNames.has(entry.phase)) {
      fail('IMPOSSIBLE_MAPPING', `phase animation references unknown phase ${entry.phase}`);
      continue;
    }
    if (entry.action < 0 || entry.action >= catalog.length) {
      fail('ACTION_OUT_OF_RANGE', `${entry.phase} maps to action ${entry.action}, catalog has ${catalog.length}`);
      continue;
    }
    mappedPhases.add(entry.phase);
    if (!actionPhases.has(entry.action)) actionPhases.set(entry.action, new Set());
    actionPhases.get(entry.action).add(entry.phase);
  }
  for (const phase of phases) {
    if (!mappedPhases.has(phase.name)) fail('MISSING_PHASE', `${phase.name} has no catalog mapping`);
  }

  const rows = catalog.map(entry => {
    const expected = manifestFiles?.[entry.file];
    const actual = hashes?.[entry.file] ?? null;
    let hash;
    if (!expected) { hash = 'UNLISTED'; fail('MISSING_MANIFEST', `${entry.file} absent from imported/0_sources.json`); }
    else if (actual === null) { hash = 'MISSING'; fail('MISSING_FILE', `${entry.file} not present on disk`); }
    else if (actual !== expected) { hash = 'MISMATCH'; fail('HASH_MISMATCH', `${entry.file} ${actual} != ${expected}`); }
    else hash = 'RETAINED';
    const mapped = [...(actionPhases.get(entry.id) ?? [])].sort();
    return {
      id: entry.id,
      action: entry.action,
      file: entry.file,
      frames: manifestFrames?.[entry.action] ?? null,
      hash,
      phases: mapped,
      selected: mapped.length > 0,
    };
  });

  return { rows, errors };
}

export function renderRows({ rows, errors }) {
  const header = ['id', 'action', 'file', 'frames', 'hash', 'phases', 'selected'];
  const widths = [2, 13, 17, 6, 10, 18, 8];
  const line = values => values.map((value, index) =>
    index === 0 || index === 3 ? String(value).padStart(widths[index]) : String(value).padEnd(widths[index])).join(' ');
  const lines = ['FALCON', line(header)];
  for (const row of rows) {
    lines.push(line([
      row.id,
      row.action,
      row.file,
      row.frames ?? '?',
      row.hash,
      row.phases.length ? row.phases.join(',') : '-',
      row.selected ? 'selected' : 'unselected',
    ]));
  }
  const selected = rows.filter(row => row.selected).length;
  lines.push(`catalog ${rows.length}; selected ${selected}; unselected ${rows.length - selected}; failures ${errors.length}`);
  for (const error of errors) lines.push(`  ${error.code}: ${error.message}`);
  return lines.join('\n');
}

export function joinExport(assembled, manifest, hashes) {
  return joinStatus({
    catalog: assembled.catalog,
    phases: assembled.phases,
    phaseAnimation: assembled.phase_animation,
    manifestFiles: manifest.files,
    manifestFrames: manifest.frames,
    hashes,
  });
}

async function main() {
  const assembled = runExport();
  const manifest = await loadManifest();
  const hashes = await fileHashes(assembled.catalog.map(entry => entry.file));
  const result = joinExport(assembled, manifest, hashes);
  console.log(renderRows(result));
  if (result.errors.length) {
    console.error(`${result.errors.length} status failures`);
    process.exitCode = 1;
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
