import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createWebApp } from '../../../../../tools/godot-web/1_web.mjs';
import { describe, head, profile, root } from './0_profile.mjs';

export const { prepare, publishCommands, serve, main } = createWebApp({
  root, describe, head, profile, packageName: 'smash_sim',
  requiredSprites: ['pixelfrog/ninjafrog/idle.png', 'kenney/zombie/zombie_idle.png',
    'falcon/idle_strip6.png', 'lucas/idle_strip30.png'],
  webAssets: [['0_push.js', 'push.js'], ['1_sw.js', 'sw.js'], ['2_turn-probe.js', 'turn-probe.js'],
    ['../../../tools/pose-capture/index.html', 'poses/index.html']],
});

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv[2], process.argv[3]).catch(error => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
