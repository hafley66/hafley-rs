import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
const fixtures = new URL('../fixtures/falcon/', import.meta.url);
const manifest = JSON.parse(readFileSync(new URL('10_lifecycle_sources.json', fixtures)));
for (const [name, hash] of Object.entries(manifest.files)) {
  assert.equal(createHash('sha256').update(readFileSync(new URL(name, fixtures))).digest('hex'), hash, name);
}
console.error(`IMPORT_SOURCES_OK files=${Object.keys(manifest.files).length}`);
