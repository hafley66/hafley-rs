import { readFile, writeFile, readdir, realpath, stat } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { dirname, resolve, relative, isAbsolute } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';

// Reuse the installed, lockfile-pinned 1.10 compiler. No second toolchain.
const require = createRequire(new URL('../blender-godot-sqlite-proof/falcon-lab/contracts/package.json', import.meta.url));
const compiler = await import(pathToFileURL(require.resolve('@typespec/compiler')));
// The pinned compiler exposes /ast only under the ESM import condition.
const { SyntaxKind, visitChildren } = await import(new URL('./ast/index.js', pathToFileURL(require.resolve('@typespec/compiler'))));
export const root = fileURLToPath(new URL('../', import.meta.url));
const source = fileURLToPath(new URL('1_registry.tsp', import.meta.url));

export async function loadRegistry(path = source) {
  const program = await compiler.compile(compiler.NodeHost, path, { noEmit: true });
  const errors = program.diagnostics.filter(d => d.severity === 'error');
  if (errors.length) throw Error(errors.map(d => `${d.code}: ${d.message}`).join('\n'));
  const node = program.sourceFiles.get(path)?.statements.find(n =>
    n.kind === SyntaxKind.ConstStatement && n.id.sv === 'entries');
  if (!node) throw Error('missing entries constant');
  // The compiler accepts repeated object keys. Registry identity must be unique.
  function unique(current) {
    if (current.kind === SyntaxKind.ObjectLiteral) {
      const names = new Set();
      for (const property of current.properties) {
        if (!property.id) throw Error('registry object spreads require explicit identity review');
        if (names.has(property.id.sv)) throw Error(`duplicate registry key: ${property.id.sv}`);
        names.add(property.id.sv);
      }
    }
    visitChildren(current, unique);
  }
  unique(node.value);
  // Same pinned internal-checker seam already used by contracts/0_constants.mjs.
  const value = program.checker.getValueForNode(node);
  return compiler.serializeValueAsJson(program, value, value.type);
}

export function localPath(base, path) {
  if (typeof path !== 'string' || !path || isAbsolute(path) || path.includes('\\')) {
    throw Error(`expected repository-relative path: ${path}`);
  }
  const resolved = resolve(base, path);
  const rel = relative(base, resolved);
  if (!rel || rel === '..' || rel.startsWith('../')) throw Error(`path escapes scope: ${path}`);
  return resolved;
}

async function existing(base, path) {
  const candidate = localPath(base, path);
  const actual = await realpath(candidate);
  localPath(await realpath(base), relative(await realpath(base), actual));
  if (!(await stat(actual)).isFile()) throw Error(`expected file: ${path}`);
  return actual;
}

export function cargoMetadata(manifest) {
  return JSON.parse(execFileSync('cargo', [
    'metadata', '--no-deps', '--offline', '--format-version', '1', '--manifest-path', manifest,
  ], { encoding: 'utf8', timeout: 20000, maxBuffer: 8 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] }));
}

export async function manifests(base, dir = base) {
  const result = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    if (entry.name.startsWith('.') || ['target', 'node_modules', 'vendor'].includes(entry.name)) continue;
    const path = resolve(dir, entry.name);
    if (entry.isDirectory()) result.push(...await manifests(base, path));
    else if (entry.isFile() && entry.name === 'Cargo.toml') result.push(relative(base, path));
  }
  return result.sort();
}

export async function validateRegistry(entries, base = root, metadata = cargoMetadata) {
  const seen = new Set();
  const tasks = await readFile(resolve(base, '3_tasks.md'), 'utf8');
  for (const [name, entry] of Object.entries(entries)) {
    if (!entry.scope.trim()) throw Error(`${name}: empty scope`);
    if (!tasks.includes(`| ${entry.task} |`)) throw Error(`${name}: unknown task ${entry.task}`);
    if (entry.stage >= 2.7 && !entry.evidence.length) throw Error(`${name}: qualification requires evidence`);
    for (const path of entry.evidence) await existing(base, path);
    for (const target of entry.targets) localPath(base, target);
    const libraries = entry.targets.filter(t => /^crates\/[A-Za-z0-9_-]+$/.test(t));
    const app = entry.targets.filter(t => t === 'smash');
    if (entry.destination === 'library' && (!libraries.length || libraries.length !== entry.targets.length)) {
      throw Error(`${name}: library targets must be direct crates/* directories`);
    }
    if (entry.destination === 'app' && (app.length !== 1 || entry.targets.length !== 1)) throw Error(`${name}: app target must be smash`);
    if (entry.destination === 'split' && (!libraries.length || !app.length || libraries.length + app.length !== entry.targets.length)) {
      throw Error(`${name}: split requires explicit library and app targets`);
    }
    if (!entry.manifest) {
      if (entry.stage > 1) throw Error(`${name}: implemented stage requires manifest`);
      continue;
    }
    const path = await existing(base, entry.manifest);
    if (seen.has(entry.manifest)) throw Error(`duplicate manifest: ${entry.manifest}`);
    seen.add(entry.manifest);
    const data = metadata(path);
    const pkg = data.packages.find(p => resolve(p.manifest_path) === path);
    if (!pkg || pkg.name !== name) throw Error(`${name}: Cargo package mismatch`);
    if (!pkg.targets.length) throw Error(`${name}: no Cargo targets`);
    for (const target of pkg.targets) await existing(base, relative(base, target.src_path));
    if (entry.stage === 4 && (entry.destination === 'split' || entry.destination === 'undecided' ||
      !entry.targets.includes(relative(base, dirname(path))))) {
      throw Error(`${name}: stage 4 requires integration at its destination`);
    }
  }
  const uncovered = (await manifests(base)).filter(path => !seen.has(path));
  if (uncovered.length) throw Error(`unclassified Cargo manifests: ${uncovered.join(', ')}`);
  return entries;
}

export function renderD2(entries) {
  const stages = [[0, 'Strawperson'], [1, 'Proposal'], [2, 'Draft'], [2.7, 'Testing'], [3, 'Candidate'], [4, 'Finished']];
  const colors = ['#F1F5F9', '#FEF3C7', '#FFEDD5', '#E0F2FE', '#D1FAE5', '#BBF7D0'];
  const lines = ['# Generated from 1_registry.tsp. Do not edit.', 'label: "05 · CHECKED CRATE INVENTORY / TC39-style promotion stages"', 'grid-columns: 1', 'legend: {', 'grid-columns: 6'];
  stages.forEach(([stage, label], i) => lines.push(`s${i}: ${JSON.stringify(`${stage} · ${label}`)} {style.fill: "${colors[i]}"; style.font-color: "#17202A"; style.stroke: "#64748B"}`));
  lines.push('}');
  for (const destination of ['library', 'split', 'app', 'undecided']) {
    const group = Object.entries(entries).filter(([, e]) => e.destination === destination);
    if (!group.length) continue;
    lines.push(`${destination}: ${JSON.stringify(`${destination.toUpperCase()} DESTINATION · ${group.length} records`)} {`, 'grid-columns: 3');
    for (const [name, e] of group) {
      const index = stages.findIndex(([stage]) => stage === e.stage);
      const label = `${name}\n${e.stage} · ${stages[index][1]} · ${e.task}\n${e.manifest ? 'Cargo reference checked' : 'PROPOSAL · crate absent'}\n→ ${e.targets.join(' + ')}`;
      lines.push(`${JSON.stringify(name)}: ${JSON.stringify(label)} {style.fill: "${colors[index]}"; style.font-color: "#17202A"; style.stroke: "#64748B"}`);
    }
    lines.push('}');
  }
  return lines.join('\n') + '\n';
}

export async function output(path, content, check) {
  if (check) {
    if (await readFile(path, 'utf8').catch(() => '') !== content) throw Error(`stale generated output: ${path}`);
  } else await writeFile(path, content);
}

async function main() {
  const mode = process.argv[2] ?? 'check';
  if (!['check', 'generate', 'status'].includes(mode)) throw Error('usage: 2_registry.mjs check|generate|status');
  const entries = await validateRegistry(await loadRegistry());
  if (mode === 'status') {
    for (const [name, e] of Object.entries(entries)) console.log(`${e.stage}\t${e.destination}\t${name}\t${e.task}\t${e.manifest ? 'present' : 'proposed'}`);
    return;
  }
  const check = mode === 'check';
  await output(new URL('3_registry.json', import.meta.url), JSON.stringify(entries, null, 2) + '\n', check);
  await output(new URL('3_registry.d2', import.meta.url), renderD2(entries), check);
  console.log(`${Object.keys(entries).length} classifications validated; outputs ${check ? 'current' : 'generated'}`);
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
