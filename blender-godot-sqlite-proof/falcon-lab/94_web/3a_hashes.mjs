import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';

export function hashes(directory) {
  return Object.fromEntries(readdirSync(directory).filter(f => f !== 'verified.json' && !f.startsWith('.')).sort().map(f =>
    [f, createHash('sha256').update(readFileSync(join(directory, f))).digest('hex')]));
}
