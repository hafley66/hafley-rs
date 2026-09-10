import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm, readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { root, loadRegistry, output } from './2_registry.mjs';
import { AXES, COVERAGE, escape, loadProgress, validateProgress, ingestRows, renderProgress } from './5_progress.mjs';

const entries = await loadRegistry();
const progress = await loadProgress();
const ingested = await ingestRows(root, progress.ingest);
const view = { ...progress, entries, ...ingested };
const rendered = renderProgress(view);

const cells = (html, key) => html.match(new RegExp(`<tr><th scope="row">${key}</th>(.*?)</tr>`, 's'))[1]
  .split(/<\/td>/).slice(0, -1).map(cell => cell.replace(/^<td>/, ''));

const fixture = async body => {
  const dir = await mkdtemp(join(tmpdir(), 'game-progress-'));
  try { return await body(dir); } finally { await rm(dir, { recursive: true }); }
};

test('committed dashboard is current and rendering is deterministic', async () => {
  assert.equal(rendered, renderProgress({ ...progress, entries, ...await ingestRows(root, progress.ingest) }));
  assert.equal(rendered, await readFile(new URL('6_progress.html', import.meta.url), 'utf8'));
  assert.equal(ingested.rows.filter(row => row.state !== 'retained').length, 0);
  assert.deepEqual(Object.keys(ingested), ['manifest', 'rows'], 'no untracked directory may reach the output');
  assert.match(rendered, /Source-game total<\/span> <span class="chip warn">UNKNOWN, unmeasured/);
  assert.equal(rendered.match(/UNMEASURED, not exported/g).length, ingested.rows.length);
  assert.match(rendered, /<nav><a href="#ingest">1 · Retained ingest \(23 files\)<\/a>/);
  assert.doesNotMatch(rendered.split('</style>')[1], /\d+\s?%/, 'no single percentage may summarize the axes');
  assert.match(rendered, /declared values, not a fresh decode/);
});

test('every rendered field is HTML escaped', async () => {
  const payload = '<script>alert("x&y")</script>';
  const hostile = structuredClone(progress);
  hostile.mechanics.walk.family = payload;
  hostile.mechanics.walk.gate = payload;
  hostile.ingest.character = payload;
  hostile.ingest.catalogApi = payload;
  hostile.references[0].note = payload;
  const html = renderProgress({ ...hostile, entries, ...ingested });
  assert.equal(html.match(/&lt;script&gt;alert\(&quot;x&amp;y&quot;\)&lt;\/script&gt;/g).length, 5);
  assert.doesNotMatch(html, /<script>/);
  assert.equal(escape(`<a href="x">&'`), '&lt;a href=&quot;x&quot;&gt;&amp;&#39;');
});

test('stale generated output is rejected without overwriting it', () => fixture(async dir => {
  const path = join(dir, '6_progress.html');
  await output(path, rendered, false);
  await assert.rejects(output(path, `${rendered}<!-- drift -->`, true), /stale generated output/);
  assert.equal(await readFile(path, 'utf8'), rendered);
}));

test('missing evidence, package and task references are rejected', async () => {
  const cases = [
    [p => { p.mechanics.walk.package = 'game-absent'; }, /walk: unknown package/],
    [p => { p.mechanics.walk.task = 'NEVER'; }, /walk: unknown task/],
    [p => { p.mechanics.walk.evidence = ['crates/fighter/absent.md']; }, /ENOENT/],
    [p => { p.mechanics.walk.evidence = ['crates/fighter']; }, /expected file/],
    [p => { p.mechanics.walk.evidence = []; }, /walk: claimed coverage requires evidence/],
    [p => { p.mechanics.walk.gate = ' '; }, /walk: empty next gate/],
    [p => { p.mechanics.walk.family = ''; }, /walk: empty family/],
    [p => { p.mechanics = {}; }, /mechanics: empty matrix/],
    [p => { p.ingest.package = 'ghost'; }, /ingest: unknown package/],
    [p => { p.ingest.task = 'NEVER'; }, /ingest: unknown task/],
    [p => { p.ingest.catalogApi = ''; }, /reported API state/],
    [p => { p.ingest.manifest = 'smash/src/fighters/falcon/imported/absent.json'; }, /ENOENT/],
    [p => { p.ingest.tests[0].note = ''; }, /ingest test: reference needs a label and a note/],
    [p => { p.references[0].path = 'crates/fighter/absent.svg'; }, /ENOENT/],
    [p => { p.references = []; }, /references: no linked evidence/],
  ];
  for (const [mutate, pattern] of cases) {
    const candidate = structuredClone(progress);
    mutate(candidate);
    await assert.rejects(validateProgress(candidate, entries, root), pattern);
  }
  assert.equal(await validateProgress(structuredClone(progress), entries, root).then(() => 'accepted'), 'accepted');
});

test('missing and mismatched retained hashes are reported as failures', () => fixture(async dir => {
  const imported = join(dir, 'imported');
  await mkdir(imported);
  await writeFile(join(imported, 'Wait1.html'), 'kept');
  await writeFile(join(imported, 'Dash.html'), 'edited');
  await writeFile(join(imported, '0_sources.json'), JSON.stringify({
    files: {
      'Wait1.html': createHash('sha256').update('kept').digest('hex'),
      'Dash.html': createHash('sha256').update('original').digest('hex'),
      'Turn.html': 'f'.repeat(64),
    },
    frames: { Wait1: 61 },
  }));
  const ingest = { ...structuredClone(progress.ingest), manifest: 'imported/0_sources.json' };
  const { rows } = await ingestRows(dir, ingest);
  assert.deepEqual(rows.map(row => [row.action, row.state, row.frames]), [
    ['Wait1', 'retained', 61], ['Dash', 'mismatch', null], ['Turn', 'missing', null],
  ]);
  const section = renderProgress({ ...progress, ingest, entries, manifest: {}, rows }).split('<section')[1];
  assert.equal(section.match(/FAILED, hash mismatch/g).length, 1);
  assert.equal(section.match(/FAILED, file absent/g).length, 1);
  assert.equal(section.match(/UNKNOWN, no manifest count/g).length, 2);
  assert.equal(section.match(/UNMEASURED, not exported/g).length, 3);
}));

test('red marks only observed failures; unproven dispositions stay yellow', () => {
  assert.deepEqual(Object.entries(COVERAGE).map(([key, [tone]]) => [key, tone]), [
    ['qualified', 'ok'], ['partial', 'warn'], ['pending', 'warn'], ['unqualified', 'warn'], ['absent', 'off'],
  ]);
  assert.equal(rendered.match(/class="chip bad"/g).length, 1, 'only the legend swatch is red');
  assert.equal(rendered.split('<section')[1].match(/class="chip bad"/g), null, 'no section reports a failure');
  assert.match(rendered, /red marks only an observed failure/);
});

test('the matrix keeps inventory, chart, live, restore and fidelity distinct', () => {
  assert.deepEqual(AXES.map(([axis]) => axis), ['inventory', 'chart', 'live', 'restore', 'fidelity']);
  const axes = key => AXES.map(([axis]) => progress.mechanics[key][axis]);
  assert.deepEqual(axes('crouch'), ['qualified', 'qualified', 'partial', 'partial', 'unqualified']);
  assert.deepEqual(axes('run'), ['qualified', 'qualified', 'qualified', 'qualified', 'unqualified']);
  assert.deepEqual(axes('grab'), ['qualified', 'absent', 'absent', 'absent', 'unqualified']);
  assert.equal(Object.values(progress.mechanics).filter(m => m.fidelity === 'qualified').length, 0,
    'no family may claim source-game fidelity while PM3.6 equivalence is unresolved');
  const chips = key => cells(rendered, key).slice(2, 7).map(cell => cell.match(/>([A-Z]+)</)[1]);
  assert.deepEqual(chips('crouch'), ['QUALIFIED', 'QUALIFIED', 'PARTIAL', 'PARTIAL', 'UNQUALIFIED']);
  assert.deepEqual(chips('run'), ['QUALIFIED', 'QUALIFIED', 'QUALIFIED', 'QUALIFIED', 'UNQUALIFIED']);
  assert.deepEqual(chips('ledge-support'), ['QUALIFIED', 'ABSENT', 'ABSENT', 'ABSENT', 'UNQUALIFIED']);
  assert.match(cells(rendered, 'game-fighter')[0], /2\.7 · Testing/);
  assert.match(cells(rendered, 'game-fighter')[6], /2\.7 -&gt; 3: declared qualification matrix/);
});
