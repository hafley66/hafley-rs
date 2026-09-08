// Standard rsync/SSH publication guarded by browser evidence and protected hashes.
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';
import { app, root } from './3_deploy.mjs';
import { verified } from './3b_verified.mjs';

const directory = resolve(root, 'build/web-game3');
const { selected, receipt } = verified(directory);
const remote = command => execFileSync('ssh', ['-o', 'BatchMode=yes', '-o', 'ConnectTimeout=15',
  selected.REMOTE_HOST, command], { encoding: 'utf8' }).trim();
const protectedHashes = () => remote('find /var/www/smash-godot -type f -exec sha256sum {} + | sort');
assert.equal(remote('readlink -f /var/www/smash-godot-game3'), '/var/www/smash-godot-game3');
assert.equal(remote('readlink -f /var/www/smash-godot'), '/var/www/smash-godot');
const before = protectedHashes();
assert(before.length > 0);
const backup = '/var/www/smash-godot-game3-backup-' + new Date().toISOString().replace(/[^0-9]/g, '');
remote(`test ! -e ${backup} && cp -a /var/www/smash-godot-game3 ${backup}`);
console.log(`Backup: ${backup}`);
try {
  for (const [command, ...args] of app.publishCommands(selected)) {
    const delayed = args.map(a => a === '--delete' ? '--delete-after' : a);
    if (command === 'rsync') delayed.unshift('--delay-updates', '--exclude=/verified.json');
    execFileSync(command, delayed, { stdio: 'inherit' });
  }
  for (const [name, expected] of Object.entries(receipt.hashes)) {
    assert.match(name, /^[a-zA-Z0-9_.-]+$/);
    assert.equal(remote(`sha256sum /var/www/smash-godot-game3/${name}`).split(/\s/)[0], expected);
  }
  assert.equal(protectedHashes(), before, 'protected game changed');
  execFileSync(process.execPath, [resolve(root, '4_browser.mjs'), '--production'], { stdio: 'inherit' });
  assert.equal(protectedHashes(), before, 'protected game changed during production verification');
  writeFileSync(resolve(root, 'build/published.json'), JSON.stringify({
    url: selected.PROBE_URL, backup, hashes: receipt.hashes, protected_unchanged: true,
  }, null, 2));
  console.log('PUBLISH_OK https://hafley.codes/game3/ protected_game=unchanged');
} catch (error) {
  remote(`rsync -a --delete ${backup}/ /var/www/smash-godot-game3/`);
  assert.equal(protectedHashes(), before, 'protected game changed');
  throw new Error('Publication verification failed; previous Game3 restored', { cause: error });
}
