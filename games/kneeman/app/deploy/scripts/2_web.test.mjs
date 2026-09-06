import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { once } from 'node:events';
import { test } from 'node:test';
import vm from 'node:vm';
import { describe, head, profile, root } from './0_profile.mjs';
import { prepare, publishCommands, serve } from './1_web.mjs';

test('profile exports, worker clients and loopback probes remain isolated', async () => {
  const values = ['game', 'game3', 'local'].map(profile);
  assert.deepEqual(values.map(p => [p.BASE_PATH, p.SERVICE_WORKER_SCOPE, p.PROBE_URL, p.REMOTE_DIR, p.EXPORT_DIR]), [
    ['/game/', '/game/', 'https://hafley.codes/game/', '/var/www/smash-godot/', 'build/web'],
    ['/game3/', '/game3/', 'https://hafley.codes/game3/', '/var/www/smash-godot-game3/', 'build/web-game3'],
    ['/game3/', '/game3/', 'http://127.0.0.1:8787/game3/', '', 'build/web-local'],
  ]);
  assert.throws(() => profile('../game'), /Unknown profile/);
  assert.throws(() => publishCommands(values[2]), /cannot publish/);
  assert.deepEqual(values.slice(0, 2).map(p => publishCommands(p)[0].at(-1)), [
    'root@hafley.codes:/var/www/smash-godot/', 'root@hafley.codes:/var/www/smash-godot-game3/',
  ]);
  for (const selected of values) {
    const directory = mkdtempSync(resolve(tmpdir(), 'kneeman-web-contract-'));
    let server;
    try {
      writeFileSync(resolve(directory, 'index.html'), '<!doctype html><html><head></head><body>Godot fixture</body></html>');
      writeFileSync(resolve(directory, 'index.wasm'), new Uint8Array([0, 97, 115, 109]));
      prepare(selected, directory);
      assert.deepEqual(JSON.parse(readFileSync(resolve(directory, 'profile.json'))), selected);
      server = serve(selected, directory);
      server.listen(0, '127.0.0.1');
      await once(server, 'listening');
      const origin = `http://127.0.0.1:${server.address().port}`;
      const response = await fetch(origin + selected.BASE_PATH);
      const html = await response.text();
      assert.deepEqual([response.status, response.headers.get('content-type'),
        response.headers.get('cross-origin-opener-policy'), response.headers.get('cross-origin-embedder-policy'),
        html.includes(head(selected))], [200, 'text/html', 'same-origin', 'require-corp', true]);
      const wasm = await fetch(origin + selected.BASE_PATH + 'index.wasm');
      assert.deepEqual([wasm.status, wasm.headers.get('content-type'), [...new Uint8Array(await wasm.arrayBuffer())]],
        [200, 'application/wasm', [0, 97, 115, 109]]);
      const other = selected.BASE_PATH === '/game/' ? '/game3/' : '/game/';
      assert.equal((await fetch(origin + other)).status, 404);
      const capture = await fetch(origin + selected.BASE_PATH + 'poses/');
      assert.equal(capture.status, 200);
      assert.equal((await capture.text()).includes('id="photos"'), true);
      assert.equal((await fetch(origin + selected.BASE_PATH + '%2e%2e%2fprofile.json')).status, 404);

      // Execute the copied V1 worker with browser API stubs. A /game3/ notification
      // must never focus a /game/ client, including when both tabs are open.
      const events = {};
      const calls = [];
      const scope = new URL(selected.BASE_PATH, 'https://hafley.codes').href;
      let clients = [{ url: 'https://hafley.codes' + other, focus: () => calls.push('wrong') },
        { url: scope + '?room=test', focus: () => calls.push('focus') }];
      vm.runInNewContext(readFileSync(resolve(directory, 'sw.js'), 'utf8'), {
        self: { registration: { scope }, addEventListener: (name, fn) => { events[name] = fn; },
          clients: { matchAll: async () => clients, openWindow: url => calls.push(url) } },
      });
      let pending;
      const click = { notification: { close() {}, data: { room: 'two words' } }, waitUntil: value => { pending = value; } };
      events.notificationclick(click);
      await pending;
      clients = clients.slice(0, 1);
      events.notificationclick(click);
      await pending;
      assert.deepEqual(calls, ['focus', scope + '?room=two%20words']);

      // Execute registration path; no VAPID/subscribe request leaves this VM.
      const registrations = [];
      const window = { SMASH_WEB_PROFILE: { basePath: selected.BASE_PATH, scope: selected.SERVICE_WORKER_SCOPE }, PushManager: {} };
      vm.runInNewContext(readFileSync(resolve(directory, 'push.js'), 'utf8'), {
        window, location: { search: '' }, URLSearchParams, console, atob,
        Notification: { requestPermission: async () => 'denied' },
        fetch: async () => ({ json: async () => ({ publicKey: 'test' }) }),
        navigator: { serviceWorker: {
          register: async (...args) => { registrations.push(args); return {}; },
          getRegistration: async scope => { registrations.push(['lookup', scope]); }, ready: Promise.resolve(),
        } },
      });
      await window.smashPush.enable();
      assert.deepEqual(JSON.parse(JSON.stringify(registrations)), [
        ['lookup', selected.SERVICE_WORKER_SCOPE],
        [selected.BASE_PATH + 'sw.js', { scope: selected.SERVICE_WORKER_SCOPE }],
      ]);
      console.log(`PASS loopback export and worker scope: ${describe(selected)}`);
    } finally {
      if (server) await new Promise(resolve => server.close(resolve));
      rmSync(directory, { recursive: true });
    }
  }
  const nginx = readFileSync(resolve(root, 'deploy/nginx/1_game3.conf'), 'utf8');
  assert.equal(nginx.includes('alias /var/www/smash-godot-game3/;'), true);
  assert.equal(nginx.includes('location /game/'), false);
});
