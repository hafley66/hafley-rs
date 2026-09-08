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
const output = mkdtempSync(join(tmpdir(), 'falcon-browser-'));
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
    window.FALCON_TRANSITIONS = [];
    Object.defineProperty(window, 'FALCON_META', {
      get: () => meta,
      set: next => {
        if (meta?.action !== next.action) window.FALCON_TRANSITIONS.push([window.FALCON_STATUS.simulation_tick, next.action]);
        meta = next;
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
  await Promise.race([page.waitForFunction(() => window.FALCON_META?.hits === 1, null, { timeout: 90000 }), runtimeFailure]);
  await page.screenshot({ path: join(output, '1_hit.png') });
  await page.waitForFunction(() => window.FALCON_META?.action === 5, null, { timeout: 30000 });
  await page.screenshot({ path: join(output, '1a_landing.png') });
  await page.waitForFunction(() => window.FALCON_STATUS?.simulation_tick === 299, null, { timeout: 30000 });
  if (!logs.some(s => s.includes('CONTROL_CAPTURE_OK'))) {
    await Promise.race([page.waitForEvent('console', {
      predicate: msg => msg.text().includes('CONTROL_CAPTURE_OK'), timeout: 30000,
    }), runtimeFailure]);
  }
  assert(logs.some(s => s.includes('CONTROL_OK ticks=300 hits=1 damage=18 replayed=120 rows_and_mesh=exact native_rows=exact')));
  assert(logs.some(s => s.includes('CONTROL_CAPTURE_OK')));
  const transitions = await page.evaluate(() => window.FALCON_TRANSITIONS);
  assert.deepEqual(transitions, [
    [0,0], [60,3], [64,1], [74,2], [114,4], [117,6], [120,0],
    [180,3], [184,1], [210,2], [237,5], [256,0], [270,3], [274,1],
  ]);
  await page.screenshot({ path: join(output, '2_replay.png') });
  await page.goto(url + '?inspect=1');
  await page.waitForFunction(() => window.FALCON_STATUS?.simulation_tick > 1, null, { timeout: 90000 });
  const start = await page.evaluate(() => window.FALCON_META.root_z);
  await page.keyboard.down('d');
  await page.waitForFunction(z => window.FALCON_META.root_z > z + 3, start);
  await page.keyboard.up('d');
  await page.waitForFunction(() => window.FALCON_STATUS.input.axis === 0);
  await page.keyboard.down('Space');
  await page.waitForFunction(() => window.FALCON_META.root_y > 0);
  await page.keyboard.up('Space');
  await page.keyboard.down('j');
  await page.waitForFunction(() => window.FALCON_META.action === 2);
  await page.keyboard.up('j');
  await page.screenshot({ path: join(output, '3_keyboard.png') });
  // Exercise the on-screen movement control through browser touch events.
  const touch = await context.newCDPSession(page);
  const z = await page.evaluate(() => window.FALCON_META.root_z);
  await touch.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: 100, y: 504 }] });
  await page.waitForFunction(z => window.FALCON_META.root_z < z - 2, z);
  await touch.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await page.waitForFunction(() => window.FALCON_STATUS.input.axis === 0);
  assert.deepEqual(errors, []);
  await context.close();
  const video = await page.video().path();
  const mp4 = join(output, 'proof.mp4');
  execFileSync('sh', [resolve(root, '../95_web.sh'), 'encode', video, mp4], { stdio: 'inherit' });
  const receipt = { url, production, native_rows: 300, replayed_states: 120,
    keyboard: true, touch: true, transitions, errors, video, mp4, output };
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
