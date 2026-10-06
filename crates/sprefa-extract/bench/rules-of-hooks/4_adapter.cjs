// One attempt for ryiii to invoke. No case loop, oracle comparison, or scoring.
// node 4_adapter.cjs CASE_FILE OUTPUT_DIRECTORY [RYII_BINARY]
const fs = require('node:fs');
const path = require('node:path');
const {spawnSync} = require('node:child_process');

const [file, directory, binary = path.join(process.env.CARGO_TARGET_DIR || path.resolve(__dirname, '../../target'), 'debug/ryii')] = process.argv.slice(2);
if (!file || !directory) throw new Error('Expected CASE_FILE OUTPUT_DIRECTORY [RYII_BINARY]');
const cases = JSON.parse(fs.readFileSync(path.join(__dirname, '1_cases.json'), 'utf8'));
const test = cases.find(test => path.resolve(__dirname, test.file) === path.resolve(file));
if (!test) throw new Error('Input must be a manifest fixture');

function command(executable, args) {
  const result = spawnSync(executable, args, {encoding: 'utf8', timeout: 55000, maxBuffer: 8 * 1024 * 1024});
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(result.stderr || `exit ${result.status}`);
  return result.stdout;
}

try {
  if (test.syntax === 'flow') throw new Error('Flow component/hook syntax is outside the TypeScript fast tier');
  const help = command(binary, ['query', '--help']);
  if (!help.includes('--scmpp')) throw new Error('ryii query lacks --scmpp; installed binary predates the checked-out source');
  fs.mkdirSync(directory, {recursive: true});
  const facts = path.resolve(directory, 'facts.db');
  const query = path.resolve(directory, 'query.db');
  // ryii refuses to overwrite stores. The caller supplies a fresh attempt directory.
  command(binary, ['--kinds', 'call,df', '--sqlite', facts, file]);
  const columns = JSON.parse(command('sqlite3', ['-json', facts, 'PRAGMA table_info(node);']));
  for (const name of ['function', 'is_async', 'owner_kind']) {
    if (!columns.some(column => column.name === name)) throw new Error(`fast node table lacks ${name}`);
  }
  command(binary, ['query', '--scmpp', path.join(__dirname, '2_hooks.scm'), '--sqlite', query, file]);
  const sql = fs.readFileSync(path.join(__dirname, '3_violations.sql'), 'utf8');
  const quote = value => `'${value.replaceAll("'", "''")}'`;
  const configuration = `CREATE TABLE bench_case(path TEXT, metadata TEXT);
INSERT INTO bench_case VALUES (${quote(file)}, ${quote(JSON.stringify({settings: test.settings || {}}))});`;
  const findings = JSON.parse(command('sqlite3', ['-json', query,
    `ATTACH ${quote(facts)} AS facts;\n${configuration}\n${sql}`]) || '[]');
  const unavailable = findings.some(finding => finding.status === 'gap');
  console.log(JSON.stringify({case_id: test.id, status: unavailable ? 'unavailable' : 'complete', findings}));
  if (unavailable) process.exitCode = 2;
} catch (error) {
  console.log(JSON.stringify({case_id: test.id, status: 'unavailable', cause: error.message, findings: null}));
  process.exitCode = 2;
}
