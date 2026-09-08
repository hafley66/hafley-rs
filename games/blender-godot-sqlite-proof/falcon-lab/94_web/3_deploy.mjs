// Reuse the existing profile validation, Emscripten/export driver and HTTP checks.
import { fileURLToPath } from 'node:url';
import { copyFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { createWebApp } from '../../../../../hafley-rs-game-runtime/tools/godot-web/1_web.mjs';
import { profile, describe } from '../../../../../hafley-rs-game-runtime/games/kneeman/app/deploy/scripts/0_profile.mjs';

export const root = fileURLToPath(new URL('.', import.meta.url));
const head = p => `<base href="${p.BASE_PATH}">\n<script>window.FALCON_DEMO = new URL(location.href).searchParams.has("demo");</script>\n`;
export const app = createWebApp({ root, describe, head, profile, packageName: 'falcon_web',
  requiredSprites: [], webAssets: [], reloadCommand: 'nginx -t' });

export function stage() {
  mkdirSync(resolve(root, 'godot/bin'), { recursive: true });
  for (const [from, to] of [
    ['2_project.godot', 'project.godot'], ['2_extension.gdextension', '0_falcon.gdextension'],
    ['2_export_presets.cfg', 'export_presets.cfg'],
    ['../godot/1_rows_auto.gd', '1_rows_auto.gd'], ['../godot/1_payload_auto.gd', '1_payload_auto.gd'],
    ['../godot/2_stage.gd', '2_stage.gd'], ['../godot/3_stage.tscn', '3_stage.tscn'],
  ]) copyFileSync(resolve(root, from), resolve(root, 'godot', to));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const action = process.argv[2];
  if (!['plan', 'export', 'check-export', 'serve'].includes(action)) throw Error('use the guarded publish recipe');
  if (action === 'export') stage();
  await app.main(action === 'serve' ? 'local' : 'game3', action);
}
