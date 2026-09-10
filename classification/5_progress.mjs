import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { dirname, resolve, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { root, loadValue, loadRegistry, validateRegistry, existing, localPath, output, taskIds } from './2_registry.mjs';

const source = fileURLToPath(new URL('4_progress.tsp', import.meta.url));
const html = new URL('6_progress.html', import.meta.url);

// Independent axes. No percentage is derived across them.
export const AXES = [
  ['inventory', 'Source inventory', 'Named in the fighter source distillation'],
  ['chart', 'Isolated chart', 'Executed statechart or decide-level tests'],
  ['live', 'Live use', 'Executed by the shipped controller path'],
  ['restore', 'Restore qualification', 'Clone plus serialized suffix replay'],
  ['fidelity', 'Source-game fidelity', 'Measured PM3.6/Melee callback equivalence'],
];

// Red is reserved for an observed failure: a broken hash or a failed check.
export const COVERAGE = {
  qualified: ['ok', 'QUALIFIED'],
  partial: ['warn', 'PARTIAL'],
  pending: ['warn', 'PENDING'],
  unqualified: ['warn', 'UNQUALIFIED'],
  absent: ['off', 'ABSENT'],
};

const RETAINED = {
  retained: ['ok', 'RETAINED, hash matches'],
  mismatch: ['bad', 'FAILED, hash mismatch'],
  missing: ['bad', 'FAILED, file absent'],
};

const STAGE_EXITS = new Map([
  [0, '0 -> 1: named problem, bounded scope, destination, task and terminal condition'],
  [1, '1 -> 2: concrete implementation/API, lifetime and storage description, consuming path'],
  [2, '2 -> 2.7: executable tests and fixtures for the scoped behavior, with listed gaps'],
  [2.7, '2.7 -> 3: declared qualification matrix passes through real consumers, including restore and target checks'],
  [3, '3 -> 4: scoped acceptance at the declared destination, splits resolved, reproducible verification and ownership'],
  [4, 'no further gate; scoped acceptance already recorded at the destination'],
]);

const STAGE_NAMES = new Map([
  [0, 'Strawperson'], [1, 'Proposal'], [2, 'Draft'], [2.7, 'Testing'], [3, 'Candidate'], [4, 'Finished'],
]);

export function escape(value) {
  return String(value).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

export function loadProgress(path = source) {
  return loadValue(path, 'progress', 'Games.Progress');
}

async function checkReference(base, entry, kind) {
  if (!entry.label.trim() || !entry.note.trim()) throw Error(`${kind}: reference needs a label and a note`);
  await existing(base, entry.path);
}

export async function validateProgress(progress, entries, base = root) {
  const tasks = await taskIds(base);
  const packages = new Set(Object.keys(entries));
  const { ingest, mechanics, references } = progress;
  if (!packages.has(ingest.package)) throw Error(`ingest: unknown package ${ingest.package}`);
  if (!tasks.has(ingest.task)) throw Error(`ingest: unknown task ${ingest.task}`);
  if (!ingest.catalogApi.trim()) throw Error('ingest: catalog membership needs a reported API state');
  await existing(base, ingest.manifest);
  for (const test of ingest.tests) await checkReference(base, test, 'ingest test');
  if (!Object.keys(mechanics).length) throw Error('mechanics: empty matrix');
  for (const [key, mechanic] of Object.entries(mechanics)) {
    if (!mechanic.family.trim()) throw Error(`${key}: empty family`);
    if (!mechanic.gate.trim()) throw Error(`${key}: empty next gate`);
    if (!packages.has(mechanic.package)) throw Error(`${key}: unknown package ${mechanic.package}`);
    if (!tasks.has(mechanic.task)) throw Error(`${key}: unknown task ${mechanic.task}`);
    if (AXES.some(([axis]) => ['qualified', 'partial'].includes(mechanic[axis])) && !mechanic.evidence.length) {
      throw Error(`${key}: claimed coverage requires evidence`);
    }
    for (const path of mechanic.evidence) await existing(base, path);
  }
  if (!references.length) throw Error('references: no linked evidence');
  for (const entry of references) await checkReference(base, entry, 'reference');
  return progress;
}

// Only tracked files are read: an untracked mirror would make output differ per checkout.
export async function ingestRows(base, ingest) {
  const manifestPath = await existing(base, ingest.manifest);
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  const dir = dirname(manifestPath);
  const rows = [];
  for (const [file, expected] of Object.entries(manifest.files ?? {})) {
    const data = await readFile(localPath(dir, file)).catch(() => null);
    const action = file.replace(/\.html$/, '');
    const frames = manifest.frames?.[action];
    const hash = data ? createHash('sha256').update(data).digest('hex') : null;
    rows.push({
      action, file, expected, hash,
      path: relative(base, resolve(dir, file)),
      state: data ? (hash === expected ? 'retained' : 'mismatch') : 'missing',
      bytes: data ? data.length : null,
      frames: Number.isInteger(frames) ? frames : null,
    });
  }
  return { manifest, rows };
}

function link(path, label) {
  return `<a href="${escape(encodeURI(`../${path}`))}">${escape(label ?? path)}</a>`;
}

function chip(map, key) {
  const [tone, text] = map[key] ?? ['off', String(key).toUpperCase()];
  return `<span class="chip ${tone}">${escape(text)}</span>`;
}

function ingestSection({ ingest, manifest, rows }) {
  const failed = rows.filter(row => row.state !== 'retained').length;
  const head = ['Action', 'Payload', 'Observed retention', 'Declared frames', 'Catalog membership'];
  const body = rows.map(row => `<tr><th scope="row">${escape(row.action)}</th>` + [
    link(row.path, row.file),
    `${chip(RETAINED, row.state)}<br><code>${escape((row.hash ?? row.expected).slice(0, 16))}</code>${row.bytes === null ? '' : ` · ${row.bytes} bytes`}`,
    row.frames === null
      ? '<span class="chip warn">UNKNOWN, no manifest count</span>'
      : `${row.frames} <span class="dim">declared</span>`,
    '<span class="chip warn">UNMEASURED, not exported</span>',
  ].map(cell => `<td>${cell}</td>`).join('') + '</tr>').join('\n');
  return `<section id="ingest">
<h2>1 · Retained ingest, ${escape(ingest.character)}</h2>
<p class="note"><span class="label">Package</span> ${escape(ingest.package)} · <span class="label">Task</span> ${escape(ingest.task)}
· <span class="label">Manifest</span> ${link(ingest.manifest, ingest.manifest)}
· <span class="label">Source</span> ${escape(manifest.source ?? 'unrecorded')} retrieved ${escape(manifest.retrieved ?? 'unrecorded')}
· <span class="label">Decoder</span> ${escape(manifest.decoder ?? 'unrecorded')}</p>
<p class="note"><span class="label">Retained payloads</span> ${rows.length}
· <span class="label">Hash failures</span> ${failed}
· <span class="label">Source-game total</span> <span class="chip warn">UNKNOWN, unmeasured</span>, so no selection fraction is shown.
Frame counts are the manifest's declared values, not a fresh decode; this generator never decodes a payload or runs the game.</p>
<p class="note"><span class="label">Catalog membership</span> ${escape(ingest.catalogApi)}</p>
<table><caption>Observed: file present, SHA256 recomputed and compared to the manifest. Declared: everything else in this table.</caption>
<thead><tr>${head.map(cell => `<th scope="col">${escape(cell)}</th>`).join('')}</tr></thead>
<tbody>
${body}
</tbody></table>
<p class="note">Declared test paths, checked for existence only. This view never executes them and cannot report a pass.<br>
${ingest.tests.map(test => `<span class="label">${escape(test.label)}</span> ${link(test.path, test.path)} — ${escape(test.note)}`).join('<br>')}</p>
</section>`;
}

function mechanicsSection(mechanics) {
  const head = ['Mechanic', 'Source family', 'Package / task', ...AXES.map(([, label]) => label), 'Declared evidence', 'Explicit next gate'];
  const body = Object.entries(mechanics).map(([key, mechanic]) => `<tr><th scope="row">${escape(key)}</th>` + [
    escape(mechanic.family),
    `${escape(mechanic.package)}<br><span class="dim">${escape(mechanic.task)}</span>`,
    ...AXES.map(([axis]) => chip(COVERAGE, mechanic[axis])),
    mechanic.evidence.map(path => link(path, path.split('/').pop())).join('<br>') || '<span class="dim">none</span>',
    escape(mechanic.gate),
  ].map(cell => `<td>${cell}</td>`).join('') + '</tr>').join('\n');
  return `<section id="mechanics">
<h2>2 · Mechanics matrix</h2>
<p class="note">Five independent axes, no combined percentage. ${AXES.map(([, label, note]) =>
    `<span class="label">${escape(label)}</span> ${escape(note)}`).join(' · ')}.
Families come from ${link('crates/fighter/4_graph.md', 'crates/fighter/4_graph.md')}; a family named there is inventory only, never a live claim.</p>
<table><caption>Every cell here is authored. Evidence paths are checked for existence; a listed path is not a passed test.</caption>
<thead><tr>${head.map(cell => `<th scope="col">${escape(cell)}</th>`).join('')}</tr></thead>
<tbody>
${body}
</tbody></table>
</section>`;
}

function packagesSection(entries) {
  const head = ['Package', 'Stage', 'Destination', 'Task', 'Cargo manifest', 'Scope and limitations', 'Declared evidence', 'Next gate'];
  const body = Object.entries(entries).map(([name, entry]) => `<tr><th scope="row">${escape(name)}</th>` + [
    `<span class="chip ${entry.stage >= 3 ? 'ok' : 'warn'}">${escape(entry.stage)} · ${escape(STAGE_NAMES.get(entry.stage) ?? 'unknown')}</span>`,
    escape(entry.destination),
    escape(entry.task),
    entry.manifest ? link(entry.manifest, entry.manifest) : '<span class="chip warn">PROPOSAL, crate absent</span>',
    escape(entry.scope),
    entry.evidence.map(path => link(path, path.split('/').pop())).join('<br>') || '<span class="dim">none</span>',
    escape(STAGE_EXITS.get(entry.stage) ?? 'unknown stage'),
  ].map(cell => `<td>${cell}</td>`).join('') + '</tr>').join('\n');
  return `<section id="packages">
<h2>3 · Checked package stages</h2>
<p class="note">Derived from ${link('classification/1_registry.tsp', 'classification/1_registry.tsp')} through the pinned compiler.
Stages are this project's TC39 adaptation and never advance automatically; this view records the authored stage and its documented exit, and claims no promotion.</p>
<table><caption>Observed: the Cargo manifest resolves. Declared: stage, scope, evidence paths. The registry checker does not evaluate test results.</caption>
<thead><tr>${head.map(cell => `<th scope="col">${escape(cell)}</th>`).join('')}</tr></thead>
<tbody>
${body}
</tbody></table>
</section>`;
}

function referencesSection(references) {
  return `<section id="references">
<h2>4 · Linked evidence</h2>
<ul>
${references.map(entry => `<li><span class="label">${escape(entry.label)}</span> ${link(entry.path, entry.path)} — ${escape(entry.note)}</li>`).join('\n')}
</ul>
</section>`;
}

export function renderProgress({ ingest, manifest, rows, mechanics, entries, references }) {
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Games port progress (generated)</title>
<style>
:root { color-scheme: dark; }
body { background: #0B1120; color: #E2E8F0; font: 15px/1.5 ui-sans-serif, system-ui, sans-serif; margin: 0 auto; max-width: 1600px; padding: 24px; }
h1 { font-size: 22px; margin: 0 0 4px; }
h2 { color: #22D3EE; font-size: 17px; margin: 32px 0 8px; }
a { color: #7DD3FC; }
code { color: #67E8F9; font-size: 12px; }
nav { border: 1px solid #1E293B; display: flex; flex-wrap: wrap; font-size: 13px; gap: 14px; margin: 10px 0 4px; padding: 6px 10px; }
table { border-collapse: collapse; width: 100%; }
caption { color: #94A3B8; font-size: 13px; padding: 4px 0; text-align: left; }
th, td { border: 1px solid #1E293B; padding: 6px 8px; text-align: left; vertical-align: top; }
thead th { background: #111827; color: #22D3EE; font-size: 13px; }
tbody th { background: #0F172A; color: #E2E8F0; font-weight: 600; white-space: nowrap; }
tbody tr:nth-child(even) td { background: #0D1526; }
td { font-size: 13px; }
.chip { border: 1px solid currentColor; border-radius: 3px; display: inline-block; font-size: 11px; font-weight: 700; padding: 1px 5px; white-space: nowrap; }
.chip.ok { color: #34D399; }
.chip.warn { color: #FBBF24; }
.chip.bad { color: #F87171; }
.chip.off { color: #94A3B8; }
.label { color: #22D3EE; font-weight: 600; }
.dim, .note { color: #94A3B8; }
.note { font-size: 13px; margin: 4px 0 10px; }
ul { padding-left: 18px; }
li { margin-bottom: 6px; }
</style>
</head>
<body>
<h1>Games port progress</h1>
<nav><a href="#ingest">1 · Retained ingest (${rows.length} files)</a>
<a href="#mechanics">2 · Mechanics matrix (${Object.keys(mechanics).length} families)</a>
<a href="#packages">3 · Package stages (${Object.keys(entries).length})</a>
<a href="#references">4 · Linked evidence</a></nav>
<p class="note">Generated by <code>just progress</code> from ${link('classification/4_progress.tsp', 'classification/4_progress.tsp')},
${link('classification/1_registry.tsp', 'classification/1_registry.tsp')} and the retained fighter manifest. Do not edit; <code>just test</code> rejects a stale copy.</p>
<p class="note"><span class="label">Observed here</span> file presence, recomputed SHA256, Cargo manifest resolution.
<span class="label">Declared here</span> every disposition, scope, frame count and evidence path; paths are checked for existence, which is not a passing test.
Colors repeat the adjacent text, and red marks only an observed failure: ${Object.keys(COVERAGE).map(key => chip(COVERAGE, key)).join(' ')} ${chip(RETAINED, 'mismatch')}</p>
${ingestSection({ ingest, manifest, rows })}
${mechanicsSection(mechanics)}
${packagesSection(entries)}
${referencesSection(references)}
</body>
</html>
`;
}

export async function buildProgress(base = root) {
  const entries = await validateRegistry(await loadRegistry(), base);
  const progress = await validateProgress(await loadProgress(), entries, base);
  return { ...progress, entries, ...await ingestRows(base, progress.ingest) };
}

async function main() {
  const mode = process.argv[2] ?? 'check';
  if (!['check', 'generate'].includes(mode)) throw Error('usage: 5_progress.mjs check|generate');
  const data = await buildProgress();
  await output(html, renderProgress(data), mode === 'check');
  const failures = data.rows.filter(row => row.state !== 'retained').length;
  console.log(`${data.rows.length} retained payloads (${failures} hash failures), ${Object.keys(data.mechanics).length} mechanic families, ${Object.keys(data.entries).length} packages; ${mode === 'check' ? 'output current' : 'output generated'}`);
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
