import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const [rootArg, fileArg, old, next] = process.argv.slice(2);
const root = resolve(rootArg);
const file = resolve(fileArg);
const source = readFileSync(file, 'utf8');
const offset = source.indexOf(old);
if (offset < 0) throw new Error(`missing ${old} in ${file}`);
const prefix = source.slice(0, offset + Math.floor(old.length / 2));
const line = prefix.split('\n').length - 1;
const character = [...prefix.split('\n').at(-1)].reduce((sum, character) => sum + character.length, 0);
const child = spawn(resolve('node_modules/typescript/bin/tsc'), ['--lsp', '--stdio'], {
  cwd: root,
  stdio: ['pipe', 'pipe', 'inherit'],
});
let input = Buffer.alloc(0);
let nextId = 1;
const pending = new Map();
child.stdout.on('data', chunk => {
  input = Buffer.concat([input, chunk]);
  while (true) {
    const split = input.indexOf('\r\n\r\n');
    if (split < 0) break;
    const header = input.subarray(0, split).toString();
    const length = Number(/Content-Length: (\d+)/i.exec(header)?.[1]);
    if (!Number.isFinite(length) || input.length < split + 4 + length) break;
    const message = JSON.parse(input.subarray(split + 4, split + 4 + length).toString());
    input = input.subarray(split + 4 + length);
    if (message.id != null && pending.has(message.id)) {
      pending.get(message.id)(message);
      pending.delete(message.id);
    } else if (message.id != null) {
      send({ jsonrpc: '2.0', id: message.id, result: null });
    }
  }
});
function send(message) {
  const body = Buffer.from(JSON.stringify(message));
  child.stdin.write(Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`), body]));
}
function request(method, params) {
  const id = nextId++;
  return new Promise((resolveResponse, reject) => {
    pending.set(id, resolveResponse);
    send({ jsonrpc: '2.0', id, method, params });
    setTimeout(() => reject(new Error(`timeout ${method}`)), 30000).unref();
  });
}
try {
  await request('initialize', {
    processId: process.pid,
    rootUri: pathToFileURL(`${root}/`).href,
    capabilities: { textDocument: { rename: { prepareSupport: true } } },
  });
  send({ jsonrpc: '2.0', method: 'initialized', params: {} });
  const uri = pathToFileURL(file).href;
  send({ jsonrpc: '2.0', method: 'textDocument/didOpen', params: {
    textDocument: { uri, languageId: file.endsWith('.tsx') ? 'typescriptreact' : 'typescript', version: 1, text: source },
  } });
  const response = await request('textDocument/rename', {
    textDocument: { uri }, position: { line, character }, newName: next,
  });
  if (response.error) throw new Error(JSON.stringify(response.error));
  const result = response.result;
  if (result?.changes) {
    result.changes = Object.fromEntries(Object.entries(result.changes)
      .map(([uri, edits]) => [relative(root, fileURLToPath(uri)), edits]));
  }
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
} finally {
  child.kill();
}
