import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { hashes } from './3a_hashes.mjs';
import { verified } from './3b_verified.mjs';

test('publication requires unchanged browser-tested assets and the isolated Game3 target', () => {
  const dir = mkdtempSync(join(tmpdir(), 'falcon-publish-test-'));
  const profile = { PROFILE: 'game3', BASE_PATH: '/game3/', PROBE_URL: 'https://hafley.codes/game3/',
    REMOTE_HOST: 'root@hafley.codes', REMOTE_DIR: '/var/www/smash-godot-game3/' };
  const json = (file, value) => writeFileSync(join(dir, file), JSON.stringify(value));
  try {
    json('profile.json', profile);
    writeFileSync(join(dir, 'index.html'), 'tested');
    assert.throws(() => verified(dir));
    const receipt = { production: false, native_rows: 300, replayed_states: 120,
      keyboard: true, touch: true, errors: [], hashes: hashes(dir) };
    json('verified.json', receipt);
    assert.deepEqual(verified(dir).selected, profile);
    writeFileSync(join(dir, 'index.html'), 'changed after test');
    assert.throws(() => verified(dir), /export changed/);
    writeFileSync(join(dir, 'index.html'), 'tested');
    json('profile.json', { ...profile, REMOTE_DIR: '/var/www/smash-godot/' });
    assert.throws(() => verified(dir));
    json('profile.json', profile);
    json('verified.json', { ...receipt, errors: ['runtime panic'] });
    assert.throws(() => verified(dir));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
