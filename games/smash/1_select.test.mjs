import test from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Pinned-compiler seam shared with classification/2_registry.mjs: the 1.10
// compiler comes from the tracked contracts package, never a second toolchain.
const require = createRequire(new URL('../blender-godot-sqlite-proof/pigeon-lab/contracts/package.json', import.meta.url));
const compiler = await import(pathToFileURL(require.resolve('@typespec/compiler')));
const { SyntaxKind } = await import(new URL('./ast/index.js', pathToFileURL(require.resolve('@typespec/compiler'))));
const source = fileURLToPath(new URL('0_select.tsp', import.meta.url));

// loadValue in the classification seam only accepts object-literal consts, so
// the array-valued roster/events are read here through the same checker.
async function load(name) {
  const program = await compiler.compile(compiler.NodeHost, source, { noEmit: true });
  const errors = program.diagnostics.filter(d => d.severity === 'error');
  if (errors.length) throw Error(errors.map(d => `${d.code}: ${d.message}`).join('\n'));
  const node = program.sourceFiles.get(source)?.statements.find(n =>
    n.kind === SyntaxKind.ConstStatement && n.id.sv === name);
  if (!node) throw Error(`missing ${name} constant`);
  const value = program.checker.getValueForNode(node);
  const type = node.type ? program.checker.getTypeForNode(node.type) : undefined;
  return compiler.serializeValueAsJson(program, value, type);
}

test('roster is exactly the current runtime identities', async () => {
  assert.deepEqual(await load('roster'), ['pigeon', 'dog']);
});

test('event shapes carry one discriminant and the slot they act on', async () => {
  assert.deepEqual(await load('events'), [
    { kind: 'join', device: 'pad-0', slot: 'p1' },
    { kind: 'choose', slot: 'p1', character: 'pigeon' },
    { kind: 'ready', slot: 'p1' },
    { kind: 'unready', slot: 'p1' },
    { kind: 'leave', slot: 'p1' },
  ]);
});

test('selection state owns device, character and ready once per slot', async () => {
  assert.deepEqual(await load('selection'), {
    slots: {
      p1: { device: 'pad-0', character: 'pigeon', ready: true },
      p2: { device: 'keyboard', character: 'dog', ready: false },
    },
  });
});
