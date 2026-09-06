import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

export const root = fileURLToPath(new URL('../../', import.meta.url));
const profiles = { game: '0_game.env', game3: '1_game3.env', local: '2_local.env' };

export function profile(name) {
  if (!Object.hasOwn(profiles, name)) throw new Error(`Unknown profile: ${name}`);
  const rows = readFileSync(resolve(root, 'deploy/profiles', profiles[name]), 'utf8');
  const value = Object.fromEntries(rows.split('\n').filter(line => line && !line.startsWith('#'))
    .map(line => [line.slice(0, line.indexOf('=')), line.slice(line.indexOf('=') + 1)]));
  const paths = { game: ['/game/', '/var/www/smash-godot/', 'build/web', 'https://hafley.codes'],
    game3: ['/game3/', '/var/www/smash-godot-game3/', 'build/web-game3', 'https://hafley.codes'],
    local: ['/game3/', '', 'build/web-local', 'http://127.0.0.1:8787'] };
  if (value.PROFILE !== name || value.BASE_PATH !== paths[name][0] ||
      value.SERVICE_WORKER_SCOPE !== value.BASE_PATH || value.REMOTE_DIR !== paths[name][1] ||
      new URL(value.PROBE_URL).pathname !== value.BASE_PATH ||
      new URL(value.PROBE_URL).origin !== paths[name][3] || value.EXPORT_DIR !== paths[name][2] ||
      value.REMOTE_HOST !== (name === 'local' ? '' : 'root@hafley.codes')) {
    throw new Error(`Invalid target isolation in profile ${name}`);
  }
  return value;
}

export function head(profile) {
  return `<base href="${profile.BASE_PATH}">\n` +
    `<script>window.SMASH_WEB_PROFILE = ${JSON.stringify({
      basePath: profile.BASE_PATH, scope: profile.SERVICE_WORKER_SCOPE,
    })};</script>\n<script src="turn-probe.js"></script>\n<script src="push.js" defer></script>\n`;
}

export function describe(profile) {
  return `profile=${profile.PROFILE} export=${profile.EXPORT_DIR} ` +
    `target=${profile.REMOTE_HOST ? profile.REMOTE_HOST + ':' + profile.REMOTE_DIR : 'loopback only'} ` +
    `base=${profile.BASE_PATH} scope=${profile.SERVICE_WORKER_SCOPE} probe=${profile.PROBE_URL}`;
}
