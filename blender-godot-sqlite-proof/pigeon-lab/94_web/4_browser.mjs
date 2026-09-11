import { chromium } from 'playwright';
import { execFileSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, appendFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { app, root } from './3_deploy.mjs';
import { hashes } from './3a_hashes.mjs';
import { profile } from '../../../../../hafley-rs-game-runtime/games/kneeman/app/deploy/scripts/0_profile.mjs';

const production = process.argv.includes('--production');
const output = mkdtempSync(join(tmpdir(), 'pigeon-browser-'));
let server;
let url = 'https://hafley.codes/game3/';
if (!production) {
  server = app.serve(profile('game3'), resolve(root, 'build/web-game3'));
  await new Promise(r => server.listen(0, '127.0.0.1', r));
  url = `http://127.0.0.1:${server.address().port}/game3/`;
}
const browser = await chromium.launch({ headless: true, args: ['--enable-unsafe-swiftshader',
  '--disable-background-timer-throttling', '--disable-renderer-backgrounding'] });
const errors = [], logs = [];
let context, page;
try {
  context = await browser.newContext({ viewport: { width: 960, height: 540 },
    recordVideo: { dir: output, size: { width: 960, height: 540 } } });
  page = await context.newPage();
  await page.addInitScript(() => {
    let meta;
    window.PIGEON_TRANSITIONS = [];
    window.PIGEON_SAMPLES = [];
    window.PIGEON_KEYS = [];
    window.addEventListener('keydown', e => window.PIGEON_KEYS.push([e.code, true]));
    window.addEventListener('keyup', e => window.PIGEON_KEYS.push([e.code, false]));
    Object.defineProperty(window, 'PIGEON_META', {
      get: () => meta,
      set: next => {
        if (meta?.action !== next.action) window.PIGEON_TRANSITIONS.push([window.PIGEON_STATUS.simulation_tick, next.action]);
        meta = next;
        if (window.PIGEON_STATUS) window.PIGEON_SAMPLES.push({
          tick: window.PIGEON_STATUS.simulation_tick, action: next.action, phase: next.phase,
          phase_ticks: next.phase_ticks, facing: next.facing, speed: next.speed,
          axis: window.PIGEON_STATUS.input.axis,
          keys: window.PIGEON_KEYS.filter(k => k[1]).map(k => k[0]).sort(),
        });
      },
    });
  });
  const runtimeFailure = new Promise((_, reject) => page.on('pageerror', reject));
  runtimeFailure.catch(() => {});
  page.on('pageerror', e => { errors.push(String(e)); console.error(String(e)); });
  page.on('requestfailed', r => { errors.push(r.url() + ' ' + r.failure()?.errorText); console.error(errors.at(-1)); });
  page.on('console', msg => {
    logs.push(msg.text());
    appendFileSync(join(output, 'browser.log'), msg.text() + '\n');
    if (msg.type() === 'error') console.error(msg.text());
    if (/panic|unreachable|^ERROR:|^SCRIPT ERROR:/.test(msg.text())) errors.push(msg.text());
  });
  await page.goto(url + '?demo=1&inspect=1');
  await Promise.race([page.waitForFunction(() => window.PIGEON_META?.hits === 1, null, { timeout: 90000 }), runtimeFailure]);
  await page.screenshot({ path: join(output, '1_hit.png') });
  await page.waitForFunction(() => window.PIGEON_META?.action === 5, null, { timeout: 30000 });
  await page.screenshot({ path: join(output, '1a_landing.png') });
  await page.waitForFunction(() => window.PIGEON_STATUS?.simulation_tick === 299, null, { timeout: 30000 });
  if (!logs.some(s => s.includes('CONTROL_CAPTURE_OK'))) {
    await Promise.race([page.waitForEvent('console', {
      predicate: msg => msg.text().includes('CONTROL_CAPTURE_OK'), timeout: 30000,
    }), runtimeFailure]);
  }
  assert(logs.some(s => s.includes('CONTROL_OK ticks=300 hits=1 damage=18 replayed=120 rows_and_mesh=exact native_rows=exact')));
  assert(logs.some(s => s.includes('CONTROL_CAPTURE_OK')));
  const transitions = await page.evaluate(() => window.PIGEON_TRANSITIONS);
  assert.deepEqual(transitions, [
    [0,0], [60,3], [64,1], [74,2], [114,4], [117,6], [120,0],
    [180,3], [184,1], [210,2], [237,5], [256,0], [270,3], [274,1],
  ]);
  await page.screenshot({ path: join(output, '2_replay.png') });
  await page.goto(url + '?inspect=1');
  await page.waitForFunction(() => window.PIGEON_STATUS?.simulation_tick > 1, null, { timeout: 90000 });
  const start = await page.evaluate(() => window.PIGEON_META.root_z);
  const dashStart = await page.evaluate(() => window.PIGEON_STATUS.simulation_tick);
  await page.keyboard.down('d');
  await page.waitForFunction(() => window.PIGEON_META.action === 10);
  // Dash dance through the real controlled path: reverse inside the initial
  // dash without releasing 'd', then alternate by re-pressing. Both DOM keys
  // are held during each overlap, so a signed sum would emit axis 0 and drop
  // the reverse; the resolver must hand the newer direction to the reducer.
  const step = async (up, down, sign) => {
    if (up) await page.keyboard.up(up);
    await page.keyboard.down(down);
    await page.waitForFunction(s => window.PIGEON_META.facing === s &&
      window.PIGEON_META.action === 10, sign);
  };
  await step(null, 'a', -1);
  await step('d', 'd', 1);
  await step('a', 'a', -1);
  await step('d', 'd', 1);
  await page.keyboard.up('a');
  const samples = await page.evaluate(() => window.PIGEON_SAMPLES);
  const danceDebug = await page.evaluate(() => window.PIGEON_PHASE_DEBUG);
  await page.screenshot({ path: join(output, '3a2_dash_dance.png') });
  await page.waitForFunction(() => window.PIGEON_META.action === 11 && window.PIGEON_META.pose >= 10);
  const running = await page.evaluate(() => ({ ...window.PIGEON_META }));
  assert.equal(running.phase, 3);
  // Reverse edge observed before Run, dash clock restarted each time, facing
  // flipped, and three-plus alternations stayed in Dash.
  const danceSamples = samples.filter(s => s.tick >= dashStart);
  const dashEntry = danceSamples.find(s => s.phase === 2 && s.facing === 1);
  assert(dashEntry, 'no right-facing Dash sample after keydown');
  assert.equal(dashEntry.axis, 1, `dash axis ${dashEntry.axis}`);
  const flips = [];
  let seenFacing = null;
  for (let i = danceSamples.indexOf(dashEntry); i < danceSamples.length; i++) {
    const s = danceSamples[i];
    if (s.phase !== 2) break;
    if (seenFacing !== null && s.facing !== seenFacing) flips.push(s);
    seenFacing = s.facing;
  }
  assert(flips.length >= 3, `facing flips inside Dash ${flips.length}`);
  for (const s of flips) {
    assert.equal(Math.sign(s.axis), Math.sign(s.facing), `axis ${s.axis} facing ${s.facing} @${s.tick}`);
    assert(Number.isFinite(s.speed), `speed ${s.speed} @${s.tick}`);
    assert(s.phase_ticks <= 6, `dash clock did not restart @${s.tick}: ${s.phase_ticks}`);
  }
  assert(danceSamples.some(s => s.keys.includes('KeyA') && s.keys.includes('KeyD')),
    'both DOM keys were never held during a reverse');
  assert(danceDebug.edges.includes('DASH->DASH'), `edges ${danceDebug.edges}`);
  await page.waitForFunction(() => window.PIGEON_PHASE_DEBUG?.active === 3);
  const phaseDebug = await page.evaluate(() => window.PIGEON_PHASE_DEBUG);
  assert(phaseDebug.edges.includes('IDLE->DASH'));
  assert(phaseDebug.edges.includes('DASH->RUN'));
  assert.equal(phaseDebug.previous, 2);
  assert(phaseDebug.nodes.includes(3));
  assert(Math.abs(running.speed - 2.3) < 0.001, `run speed ${running.speed}`);
  assert.equal(running.facing, 1);
  assert(running.root_z > start + 20);
  await page.screenshot({ path: join(output, '3b_run.png') });
  await page.keyboard.up('d');
  await page.waitForFunction(() => window.PIGEON_META.action === 0 && window.PIGEON_META.speed === 0);
  await page.keyboard.down('a');
  await page.waitForFunction(() => window.PIGEON_META.action === 11 && window.PIGEON_META.speed < -2.29);
  assert.equal(await page.evaluate(() => window.PIGEON_META.facing), -1);
  await page.screenshot({ path: join(output, '3c_left_run.png') });
  await page.keyboard.up('a');
  await page.waitForFunction(() => window.PIGEON_META.action === 0 && window.PIGEON_META.speed === 0);
  await page.keyboard.down('Shift');
  await page.keyboard.down('d');
  await page.waitForFunction(() => window.PIGEON_META.action === 8 && window.PIGEON_META.speed > 0);
  await page.keyboard.up('d');
  await page.keyboard.up('Shift');
  await page.waitForFunction(() => window.PIGEON_STATUS.input.axis === 0 &&
    window.PIGEON_META.action === 0 && window.PIGEON_META.speed === 0);
  await page.keyboard.down('Space');
  await page.waitForFunction(() => window.PIGEON_META.root_y > 0);
  await page.keyboard.up('Space');
  await page.keyboard.down('j');
  await page.waitForFunction(() => window.PIGEON_META.action === 2);
  await page.keyboard.up('j');
  await page.screenshot({ path: join(output, '3_keyboard.png') });
  // Exercise the on-screen movement control through browser touch events.
  const touch = await context.newCDPSession(page);
  const z = await page.evaluate(() => window.PIGEON_META.root_z);
  await touch.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: 100, y: 504 }] });
  await page.waitForFunction(z => window.PIGEON_META.root_z < z - 2, z);
  await touch.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await page.waitForFunction(() => window.PIGEON_STATUS.input.axis === 0);
  assert.deepEqual(errors, []);
  await context.close();
  const video = await page.video().path();
  const mp4 = join(output, 'proof.mp4');
  execFileSync('sh', [resolve(root, '../95_web.sh'), 'encode', video, mp4], { stdio: 'inherit' });
  const receipt = { url, production, native_rows: 300, replayed_states: 120,
    keyboard: true, touch: true, locomotion: { run_speed: running.speed, dash: true, run: true, left_run: true, walk: true },
    dash_dance: { dash_tick: dashStart, flips: flips.length, reversals: flips,
      overlap_keys: danceSamples.find(s => s.keys.includes('KeyA') && s.keys.includes('KeyD'))?.keys,
      samples: danceSamples.filter(s => s.phase === 2) },
    phase_debug: phaseDebug, transitions, errors, video, mp4, output };
  writeFileSync(join(output, 'receipt.json'), JSON.stringify(receipt, null, 2));
  if (!production) writeFileSync(resolve(root, 'build/web-game3/verified.json'),
    JSON.stringify({ ...receipt, hashes: hashes(resolve(root, 'build/web-game3')) }, null, 2));
  console.log(`BROWSER_OK artifacts=${output}`);
} finally {
  writeFileSync(join(output, 'browser.log'), logs.join('\n'));
  writeFileSync(join(output, 'errors.json'), JSON.stringify(errors, null, 2));
  if (page && !page.isClosed()) await page.screenshot({ path: join(output, 'last.png') }).catch(() => {});
  if (context) await context.close();
  await browser.close();
  if (server) await new Promise(r => server.close(r));
}
