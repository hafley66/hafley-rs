import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { compile, NodeHost, getMinItems, getMaxItems } from '@typespec/compiler';
import { Output, List, render, refkey } from '@alloy-js/core';
import { createComponent as c } from '@alloy-js/core/jsx-runtime';
import { stringify } from 'yaml';
import {
  CrateDirectory, SourceFile, StructDeclaration, StructField,
  EnumDeclaration, UnitVariant, TraitDeclaration, TraitMethod, TypeAlias,
} from '@hafley66/alloy-rs';

const source = new URL('0_presentation.tsp', import.meta.url);
const args = process.argv.slice(2);
if (args.some(arg => arg !== '--check')) throw Error('usage: node 1_generate.mjs [--check]');
const program = await compile(NodeHost, fileURLToPath(source), { noEmit: true });
if (program.diagnostics.some(d => d.severity === 'error')) {
  throw Error(program.diagnostics.map(d => `${d.code}: ${d.message}`).join('\n'));
}
const ns = program.getGlobalNamespaceType().namespaces.get('FalconBoundary');
if (!ns) throw Error('missing FalconBoundary namespace');
const readBuffer = ns.models.get('ReadBuffer');
const writeBuffer = ns.models.get('WriteBuffer');
const models = [...ns.models.values()].filter(m => m !== readBuffer && m !== writeBuffer);
const enums = [...ns.enums.values()];
const unions = [...ns.unions.values()];
const interfaces = [...ns.interfaces.values()];
const known = new Set([...models, ...enums, ...unions]);
const scalars = { int64: 'i64', uint64: 'u64', uint32: 'u32', float64: 'f64' };
const lines = children => c(List, { hardline: true, children });

// Fail closed. No unsupported schema kind silently becomes String or Vec.
function shape(type, property, parameter = false) {
  if (type.kind === 'Scalar' && scalars[type.name]) return { rust: scalars[type.name], type: type.name };
  if (known.has(type)) return { rust: type.name, ref: type.name };
  if (type.kind === 'Model' && type.templateMapper && (type.node === readBuffer.node || type.node === writeBuffer.node)) {
    if (!parameter) throw Error('buffers are operation parameters only');
    const item = shape(type.templateMapper.args[0]);
    const mutable = type.node === writeBuffer.node;
    return { rust: `&${mutable ? 'mut ' : ''}[${item.rust}]`, buffer: mutable ? 'write' : 'read', item };
  }
  if (type.kind === 'Model' && type.name === 'Array' && type.namespace.name === 'TypeSpec') {
    const min = getMinItems(program, property);
    const max = getMaxItems(program, property);
    if (!Number.isSafeInteger(min) || min <= 0 || min !== max) throw Error('arrays require equal positive minItems/maxItems');
    const item = shape(type.indexer.value);
    return { rust: `[${item.rust}; ${min}]`, length: min, item };
  }
  throw Error(`unsupported boundary type ${type.kind}:${type.name ?? '<anonymous>'}`);
}

const declarations = [];
const contract = { version: 1, namespace: ns.name, models: {}, enums: {}, results: {}, interfaces: {} };
for (const model of models) {
  if (model.baseModel || model.indexer) throw Error(`unsupported model composition: ${model.name}`);
  const fields = [...model.properties.values()].map(p => {
    if (p.optional || p.defaultValue) throw Error(`unsupported optional/default field: ${model.name}.${p.name}`);
    return { name: p.name, ...shape(p.type, p) };
  });
  contract.models[model.name] = fields;
  declarations.push(c(StructDeclaration, {
    name: model.name, pub: true, refkey: refkey(model),
    derive: ['Debug', 'Clone', 'Copy', 'PartialEq', 'Serialize', 'Deserialize'],
    children: lines(fields.map(f => c(StructField, { name: f.name, type: f.rust, pub: true }))),
  }));
}
for (const enumeration of enums) {
  const members = [...enumeration.members.values()];
  if (members.some(m => m.value !== undefined)) throw Error('valued enums require an explicit mapping');
  contract.enums[enumeration.name] = members.map(m => m.name);
  declarations.push(c(EnumDeclaration, {
    name: enumeration.name, pub: true, derive: ['Debug', 'Clone', 'Copy', 'PartialEq', 'Serialize', 'Deserialize'],
    children: lines(members.map(m => c(UnitVariant, { name: m.name }))),
  }));
}
for (const union of unions) {
  if (union.variants.size !== 2 || !union.variants.has('Ok') || !union.variants.has('Err')) throw Error(`unsupported union: ${union.name}`);
  const ok = shape(union.variants.get('Ok').type);
  const error = shape(union.variants.get('Err').type);
  contract.results[union.name] = { ok, error };
  declarations.push(c(TypeAlias, { name: union.name, pub: true, children: `Result<${ok.rust}, ${error.rust}>` }));
}
for (const iface of interfaces) {
  const operations = [...iface.operations.values()].map(op => ({
    name: op.name,
    params: [...op.parameters.properties.values()].map(p => {
      if (p.optional || p.defaultValue) throw Error(`unsupported optional/default parameter: ${op.name}.${p.name}`);
      return { name: p.name, ...shape(p.type, p, true) };
    }),
    returns: shape(op.returnType),
  }));
  contract.interfaces[iface.name] = operations;
  declarations.push(c(TraitDeclaration, {
    name: iface.name, pub: true,
    children: lines(operations.map(op => c(TraitMethod, {
      name: op.name, selfParam: '&mut', params: op.params.map(p => ({ name: p.name, type: p.rust })), returns: op.returns.rust,
    }))),
  }));
}
const tree = render(c(Output, { children: c(CrateDirectory, {
  children: c(SourceFile, { path: '2_presentation_auto.rs', externalUses: ['serde::Serialize', 'serde::Deserialize'], children: lines(declarations) }),
}) }));
function files(node) {
  return node.kind === 'file' ? [node] : node.contents.flatMap(files);
}
const rust = files(tree).find(f => f.path.endsWith('2_presentation_auto.rs'));
if (!rust) throw Error('Rust emitter returned no contract file');
const hash = createHash('sha256').update(await readFile(source)).digest('hex');
const outputs = new Map([
  ['2_presentation_auto.rs', `// Generated from 0_presentation.tsp; sha256:${hash}\n${rust.contents}\n`],
  ['2_presentation_auto.yaml', `# Generated from 0_presentation.tsp; sha256:${hash}\n${stringify(contract)}`],
]);
for (const [name, body] of outputs) {
  const target = new URL(name, import.meta.url);
  const prior = await readFile(target, 'utf8').catch(e => { if (e.code === 'ENOENT') return null; throw e; });
  if (body === prior) continue;
  if (args.includes('--check')) throw Error(`stale generated file: ${name}; run just generate`);
  await writeFile(target, body);
}
console.log(`CONTRACTS_OK models=${models.length} interfaces=${interfaces.length} check=${args.includes('--check')}`);
