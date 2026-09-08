import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { hashes } from './3a_hashes.mjs';

export function verified(directory) {
  const selected = JSON.parse(readFileSync(resolve(directory, 'profile.json')));
  assert.equal(selected.PROFILE, 'game3');
  assert.equal(selected.BASE_PATH, '/game3/');
  assert.equal(selected.PROBE_URL, 'https://hafley.codes/game3/');
  assert.equal(selected.REMOTE_HOST, 'root@hafley.codes');
  assert.equal(selected.REMOTE_DIR, '/var/www/smash-godot-game3/');
  const receipt = JSON.parse(readFileSync(resolve(directory, 'verified.json')));
  assert.equal(receipt.production, false);
  assert.equal(receipt.native_rows, 300);
  assert.equal(receipt.replayed_states, 120);
  assert.equal(receipt.keyboard, true);
  assert.equal(receipt.touch, true);
  assert.deepEqual(receipt.errors, []);
  assert.deepEqual(hashes(directory), receipt.hashes, 'export changed after browser verification');
  return { selected, receipt };
}
