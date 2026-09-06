import { copyFileSync, existsSync, mkdirSync, readFileSync, statSync, unlinkSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { dirname, extname, resolve, sep } from 'node:path';
import { spawnSync } from 'node:child_process';
// The consuming game owns target validation, page additions and asset requirements.
export function createWebApp({ root, describe, head, profile, packageName, requiredSprites,
  webAssets, reloadCommand = 'nginx -t && systemctl reload nginx' }) {

function run(command, args, options = {}) {
  console.log(JSON.stringify([command, ...args]));
  const child = spawnSync(command, args, { cwd: root, stdio: 'inherit', ...options });
  if (options.stdio === 'pipe') {
    if (child.stdout) process.stdout.write(child.stdout);
    if (child.stderr) process.stderr.write(child.stderr);
  }
  if (child.error) throw child.error;
  if (child.status !== 0) throw new Error(`${command} failed: status=${child.status} signal=${child.signal}`);
  return child;
}

function toolPaths(name) {
  // Only tool locations are read from the ignored override, never executable shell text.
  const path = resolve(root, `deploy/profiles/${name}.local.env`);
  const overrides = existsSync(path) ? Object.fromEntries(readFileSync(path, 'utf8').split('\n')
    .filter(line => /^(GODOT45|EMSDK_ENV)=/.test(line))
    .map(line => [line.slice(0, line.indexOf('=')), line.slice(line.indexOf('=') + 1)])) : {};
  return {
    godot: process.env.GODOT45 || overrides.GODOT45 || resolve(process.env.HOME, 'godot45/Godot.app/Contents/MacOS/Godot'),
    sdk: process.env.EMSDK_ENV || overrides.EMSDK_ENV || resolve(process.env.HOME, 'emsdk/emsdk_env.sh'),
  };
}

function wasm(profile) {
  const paths = toolPaths(profile.PROFILE);
  const toolchain = readFileSync(resolve(root, 'rust-toolchain.toml'), 'utf8')
    .match(/^channel\s*=\s*"([^"]+)"/m)?.[1];
  if (!toolchain) throw new Error('Missing pinned channel in rust-toolchain.toml');
  run('bash', ['-c', 'set -euo pipefail; source "$1"; export GODOT4_BIN="$2"; cargo build -p "$3" -Zbuild-std --target wasm32-unknown-emscripten --release --target-dir "$4"',
    'godot-wasm', paths.sdk, paths.godot, packageName, resolve(root, 'target')], {
    env: { ...process.env, RUSTUP_TOOLCHAIN: toolchain, EMSDK_QUIET: '1' },
  });
}

function prepare(profile, directory = resolve(root, profile.EXPORT_DIR)) {
  const page = resolve(directory, 'index.html');
  const html = readFileSync(page, 'utf8');
  if (!html.includes('<head>')) throw new Error('Export has no <head>');
  if (html.includes(head(profile))) throw new Error('Export already prepared; export again first');
  writeFileSync(page, html.replace('<head>', '<head>\n' + head(profile)));
  for (const [source, target] of webAssets) {
    mkdirSync(dirname(resolve(directory, target)), { recursive: true });
    copyFileSync(resolve(root, 'deploy/web', source), resolve(directory, target));
  }
  writeFileSync(resolve(directory, 'profile.json'), JSON.stringify(profile, null, 2) + '\n');
}

function exportWeb(profile) {
  const output = resolve(root, profile.EXPORT_DIR);
  const receipt = resolve(output, 'profile.json');
  if (existsSync(receipt)) unlinkSync(receipt);
  // A fresh worktree lacks ignored art. Refuse a sprite-less production export.
  const missing = requiredSprites
    .filter(file => !existsSync(resolve(root, 'godot/assets', file)));
  if (missing.length) throw new Error(`Missing boot sprites: ${missing.join(', ')}. Restore with just assets-from <assets-directory>.`);
  // The editor loads the native extension while exporting to discover Rust classes.
  run('cargo', ['build', '-p', packageName, '--target-dir', resolve(root, 'target')]);
  wasm(profile);
  mkdirSync(output, { recursive: true });
  mkdirSync(resolve(root, 'godot/bin'), { recursive: true });
  const target = resolve(root, 'target');
  copyFileSync(resolve(target, `wasm32-unknown-emscripten/release/${packageName}.wasm`), resolve(root, `godot/bin/${packageName}.wasm`));
  const exported = run(toolPaths(profile.PROFILE).godot, ['--headless', '--path', resolve(root, 'godot'),
    '--export-release', profile.GODOT_PRESET, resolve(output, 'index.html')], { encoding: 'utf8', stdio: 'pipe' });
  if (/^(?:SCRIPT )?ERROR:/m.test(exported.stdout + exported.stderr)) {
    throw new Error('Godot reported export errors despite a zero exit status');
  }
  prepare(profile);
  for (const asset of ['index.side.wasm', `${packageName}.wasm`, 'index.js', 'index.wasm', 'index.pck']) {
    if (existsSync(resolve(output, asset))) run('gzip', ['-kf', resolve(output, asset)]);
  }
}

function publishCommands(profile) {
  if (!profile.REMOTE_HOST || !profile.REMOTE_DIR) throw new Error('Local profile cannot publish');
  return [
    ['rsync', '-az', '--delete', resolve(root, profile.EXPORT_DIR) + '/', `${profile.REMOTE_HOST}:${profile.REMOTE_DIR}`],
    ['ssh', profile.REMOTE_HOST, reloadCommand],
  ];
}

function serve(profile, directory = resolve(root, profile.EXPORT_DIR)) {
  const base = resolve(directory);
  const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css',
    '.wasm': 'application/wasm', '.json': 'application/json', '.png': 'image/png', '.ico': 'image/x-icon' };
  return createServer((request, response) => {
    const headers = { 'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp', 'Cache-Control': 'no-cache' };
    try {
      const pathname = decodeURIComponent(new URL(request.url, 'http://127.0.0.1').pathname);
      if (!pathname.startsWith(profile.BASE_PATH)) throw new Error('Outside profile');
      const relative = pathname.slice(profile.BASE_PATH.length);
      const suffix = relative.endsWith('/') ? relative + 'index.html' : relative || 'index.html';
      const path = resolve(base, suffix);
      if (!path.startsWith(base + sep) || !statSync(path).isFile()) throw new Error('Not a file');
      response.writeHead(200, { ...headers, 'Content-Type': types[extname(path)] || 'application/octet-stream' });
      response.end(request.method === 'HEAD' ? undefined : readFileSync(path));
    } catch {
      response.writeHead(404, headers);
      response.end('Not found');
    }
  });
}

async function main(name, action) {
  const selected = profile(name);
  console.log(describe(selected));
  if (action === 'plan') {
    // Read committed configuration only. No builds, overrides, requests, or subprocesses.
    console.log(`build: cargo native + wasm; export: ${selected.GODOT_PRESET} -> ${selected.EXPORT_DIR}/index.html`);
    if (selected.PROFILE !== 'local') console.log(JSON.stringify(publishCommands(selected), null, 2));
    return;
  }
  if (action === 'wasm') return wasm(selected);
  if (action === 'export') return exportWeb(selected);
  if (action === 'publish') {
    const recorded = JSON.parse(readFileSync(resolve(root, selected.EXPORT_DIR, 'profile.json')));
    if (JSON.stringify(recorded) !== JSON.stringify(selected)) throw new Error('Export profile differs from publish target');
    for (const [command, ...args] of publishCommands(selected)) run(command, args);
    return;
  }
  if (action === 'dev' || action === 'serve') {
    if (selected.PROFILE !== 'local') throw new Error('Serve requires local profile');
    if (action === 'dev') exportWeb(selected);
    const url = new URL(selected.PROBE_URL);
    serve(selected).listen(Number(url.port), '127.0.0.1', () => console.log(`local: ${url}`));
    return;
  }
  if (action === 'probe') {
    if (selected.PROFILE !== 'local') throw new Error('Probe is restricted to the local profile');
    const response = await fetch(selected.PROBE_URL);
    const html = await response.text();
    if (!response.ok || response.headers.get('cross-origin-opener-policy') !== 'same-origin' ||
        response.headers.get('cross-origin-embedder-policy') !== 'require-corp' || !html.includes(head(selected))) {
      throw new Error(`Local export probe failed: ${response.status}`);
    }
    console.log(`PASS ${selected.PROBE_URL}`);
    return;
  }
  if (action === 'check-export') {
    // Serve this profile's real artifact on loopback; never fetch its production URL.
    const server = serve(selected);
    try {
      await new Promise((resolve, reject) => {
        server.once('error', reject);
        server.listen(0, '127.0.0.1', resolve);
      });
      const url = `http://127.0.0.1:${server.address().port}${selected.BASE_PATH}`;
      const response = await fetch(url);
      const html = await response.text();
      if (response.status !== 200 || !html.includes(head(selected)) ||
          response.headers.get('cross-origin-opener-policy') !== 'same-origin' ||
          response.headers.get('cross-origin-embedder-policy') !== 'require-corp') {
        throw new Error('Real export HTML or isolation headers failed');
      }
      for (const file of ['index.wasm', 'index.side.wasm', `${packageName}.wasm`, 'index.js', 'index.pck', ...webAssets.map(([, target]) => target)]) {
        const asset = await fetch(url + file, { method: 'HEAD' });
        if (asset.status !== 200 || (file.endsWith('.wasm') && asset.headers.get('content-type') !== 'application/wasm')) {
          throw new Error(`Real export asset failed: ${file} (${asset.status})`);
        }
      }
      const recorded = await (await fetch(url + 'profile.json')).json();
      if (JSON.stringify(recorded) !== JSON.stringify(selected)) throw new Error('Real export profile mismatch');
      console.log(`PASS real export on loopback: ${selected.EXPORT_DIR} base=${selected.BASE_PATH}`);
    } finally {
      await new Promise(resolve => server.close(resolve));
    }
    return;
  }
  throw new Error(`Unknown action: ${action}`);
}

return { prepare, publishCommands, serve, main };
}
