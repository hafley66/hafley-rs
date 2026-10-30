// Server RSS across phases of one TS 7.1 API session.
import { join, resolve } from "node:path";
import { execFileSync } from "node:child_process";
const pkg = process.env.TS71_PKG;
const { API } = await import(join(pkg, "dist/api/sync/api.js"));
const { SyntaxKind: K } = await import(join(pkg, "dist/ast/index.js"));
const [root, tsconfig] = process.argv.slice(2);
const server = () => Number(execFileSync("pgrep", ["-P", String(process.pid)]).toString().trim().split("\n")[0]);
const rss = pid => Math.round(Number(execFileSync("ps", ["-o", "rss=", "-p", String(pid)]).toString()) / 1024);
const log = (phase, pid, t0) => console.log(`${phase.padEnd(34)} ${String(rss(pid)).padStart(6)} MB  ${Math.round(performance.now() - t0)} ms`);
let t = performance.now();
const api = new API({ cwd: root }); api.ensureInitialized();
const pid = server(); log("spawn", pid, t);
const pass = (snap, label) => {
  const project = snap.getProjects()[0];
  const files = project.program.getSourceFileNames().filter(f => f.startsWith(root + "/") && !f.includes("/node_modules/") && /\.tsx?$/.test(f) && !f.endsWith(".d.ts"));
  let t0 = performance.now(); const idents = new Map();
  for (const f of files) {
    const sf = project.program.getSourceFile(f); const pos = [];
    const visit = n => { if (n.kind === K.Identifier) pos.push(n.getStart(sf)); n.forEachChild(visit); };
    sf.forEachChild(visit); idents.set(f, pos);
  }
  log(`${label}: ast fetch ${files.length} files`, pid, t0);
  t0 = performance.now(); let n = 0;
  for (const [f, pos] of idents) { if (pos.length) { project.checker.getSymbolAtPosition(f, pos); n += pos.length; } }
  log(`${label}: symbols ${n} identifiers`, pid, t0);
  t0 = performance.now();
  for (const [f, pos] of idents) { if (pos.length) project.checker.getSymbolAtPosition(f, pos); }
  log(`${label}: same symbols again`, pid, t0);
  t0 = performance.now();
  for (const [f, pos] of idents) { if (pos.length) project.checker.getTypeAtPosition(f, pos); }
  log(`${label}: types every identifier`, pid, t0);
};
t = performance.now(); let snap = api.createSnapshot({ openProjects: [resolve(root, tsconfig)] }); log("snapshot 1 open", pid, t);
pass(snap, "snap1");
t = performance.now(); snap.dispose(); log("snapshot 1 dispose", pid, t);
await new Promise(r => setTimeout(r, 3000)); log("+3 s idle", pid, t);
t = performance.now(); snap = api.createSnapshot({ openProjects: [resolve(root, tsconfig)] }); log("snapshot 2 open", pid, t);
pass(snap, "snap2");
snap.dispose(); log("snapshot 2 dispose", pid, t);
api.close();
