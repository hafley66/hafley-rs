import { createRequire } from 'node:module';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const packagePath = process.env.TS_CLASSIC_PACKAGE;
if (!packagePath) throw new Error('set TS_CLASSIC_PACKAGE to the TypeScript 5.9.3 or 6.x package directory');
const require = createRequire(join(resolve(packagePath), 'package.json'));
const ts = require(resolve(packagePath));
if (!/^(5\.9\.3|6\.)/.test(ts.version)) throw new Error(`classic tsserver version: ${ts.version}`);

const root = resolve(import.meta.dirname, '../tests/fixtures/ts7_api');
const cases = [
  ['0_fixture.ts', 'old', 6, '0_fixture.rename.json'],
  ['1_shorthand.ts', 'old', 6, '1_shorthand.rename.json'],
  ['2_destructure.ts', 'old', 17, '2_destructure.rename.json'],
  ['3_intrinsic.tsx', 'div', 17, '3_intrinsic.rename.json'],
  ['4_export.ts', 'old', 6, '4_export.rename.json'],
  ['4_export.ts', 'publicName', 31, '4_export.publicName.rename.json'],
  ['5_module.ts', 'old', 13, '5_module.rename.json'],
  ['6_import.ts', 'local', 16, '6_import.rename.json'],
  ['7_import_unaliased.ts', 'old', 9, '7_import_unaliased.rename.json'],
  ['8_contextual_shorthand.ts', 'old', 15, '8_contextual_shorthand.rename.json'],
  ['9_destructure_local.ts', 'old', 35, '9_destructure_local.rename.json'],
  ['10_destructure_alias.ts', 'old', 17, '10_destructure_alias.old.rename.json'],
  ['10_destructure_alias.ts', 'local', 40, '10_destructure_alias.local.rename.json'],
  ['11_reexport_mid.ts', 'mid', 16, '11_reexport_mid.rename.json'],
  ['13_reexport_consumer.ts', 'api', 9, '13_reexport_consumer.rename.json'],
  ['14_contextual_member.ts', 'old', 15, '14_contextual_member.rename.json'],
  ['15_union_context.ts', 'old', 22, '15_union_context.rename.json'],
  ['16_intersection.ts', 'old', 11, '16_intersection.rename.json'],
  ['17_overloads.ts', 'old', 9, '17_overloads.rename.json'],
  ['18_merge.ts', 'Old', 10, '18_merge.rename.json'],
  ['19_jsx_component.tsx', 'Old', 9, '19_jsx_component.rename.json'],
  ['20_string_property.ts', 'old', 20, '20_string_property.rename.json'],
  ['21_string_type.ts', 'old', 16, '21_string_type.rename.json'],
  ['22_numeric.ts', '0', 15, '22_numeric.rename.json'],
  ['24_module_path.ts', '23_path_source', 26, '24_module_path.rename.json'],
];
const files = readdirSync(root)
  .filter(name => /\.tsx?$/.test(name))
  .map(name => join(root, name));
const host = {
  getScriptFileNames: () => files,
  getScriptVersion: () => '0',
  getScriptSnapshot: file => ts.sys.fileExists(file)
    ? ts.ScriptSnapshot.fromString(readFileSync(file, 'utf8'))
    : undefined,
  getCurrentDirectory: () => root,
  getCompilationSettings: () => ({
    jsx: ts.JsxEmit.Preserve,
    allowJs: true,
    moduleResolution: ts.ModuleResolutionKind.Node10,
    target: ts.ScriptTarget.Latest,
  }),
  getDefaultLibFileName: options => ts.getDefaultLibFilePath(options),
  fileExists: ts.sys.fileExists,
  readFile: ts.sys.readFile,
  readDirectory: ts.sys.readDirectory,
};
const service = ts.createLanguageService(host);
function position(text, offset) {
  const before = text.slice(0, offset).split('\n');
  return { line: before.length - 1, character: before.at(-1).length };
}
for (const [source, old, at, oracle] of cases) {
  const file = join(root, source);
  const content = readFileSync(file, 'utf8');
  const start = content.lastIndexOf(old, at);
  if (start < 0 || at >= start + old.length) throw new Error(`${source}: bad anchor ${old} at ${at}`);
  const info = service.getRenameInfo(file, at, { allowRenameOfImportPath: true });
  const locations = info.canRename
    ? service.findRenameLocations(file, at, false, false, { providePrefixAndSuffixTextForRename: true }) || []
    : [];
  const newName = /^[A-Z]/.test(old) ? 'Next' : 'next';
  const changes = {};
  for (const location of locations) {
    const rel = location.fileName.slice(root.length + 1);
    const text = readFileSync(location.fileName, 'utf8');
    (changes[rel] ??= []).push({
      range: {
        start: position(text, location.textSpan.start),
        end: position(text, location.textSpan.start + location.textSpan.length),
      },
      newText: `${location.prefixText || ''}${newName}${location.suffixText || ''}`,
    });
  }
  const result = locations.length ? { changes } : null;
  writeFileSync(join(root, oracle), `${JSON.stringify(result, null, 2)}\n`);
  process.stdout.write(`${oracle}\t${locations.length}\n`);
}
