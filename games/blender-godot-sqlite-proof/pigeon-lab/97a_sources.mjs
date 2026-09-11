import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
let count = 0;
for (const [directory, file] of [
  ['../fixtures/pigeon/', '10_lifecycle_sources.json'],
  ['../../smash/src/fighters/pigeon/imported/', '0_sources.json'],
]) {
  const fixtures = new URL(directory, import.meta.url);
  const manifest = JSON.parse(readFileSync(new URL(file, fixtures)));
  for (const [name, hash] of Object.entries(manifest.files)) {
    assert.equal(createHash('sha256').update(readFileSync(new URL(name, fixtures))).digest('hex'), hash, name);
    count++;
  }
}
console.error(`IMPORT_SOURCES_OK files=${count}`);
