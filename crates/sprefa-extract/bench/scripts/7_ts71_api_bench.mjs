// node 7_ts71_api_bench.mjs --root DIR --out JSON --sample N [--files-out TXT] TSCONFIG...
// Bulk phases cover every corpus file; method tasks cover a fixed stride sample. RPCs are timed at Client.
import { parseArgs } from "node:util";
import { writeFileSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Worker } from "node:worker_threads";
import { execFileSync } from "node:child_process";

const here = dirname(fileURLToPath(import.meta.url));
const pkg = process.env.TS71_PKG ?? join(here, "../repos/ts71/node_modules/typescript");
const api_mod = await import(join(pkg, "dist/api/sync/api.js"));
const client_mod = await import(join(pkg, "dist/api/sync/client.js"));
const ast_mod = await import(join(pkg, "dist/ast/index.js"));
const { API, SymbolFlags, SignatureKind, IndexKind, TypeFlags, ObjectFlags, EmitOnly } = api_mod;
const { SyntaxKind } = ast_mod;
const ts_version = JSON.parse(readFileSync(join(pkg, "package.json"), "utf8")).version;

const { values: opt, positionals: tsconfigs } = parseArgs({
  allowPositionals: true,
  options: {
    root: { type: "string" }, out: { type: "string" }, sample: { type: "string", default: "200" },
    "files-out": { type: "string" }, "skip-build": { type: "boolean", default: false },
    scratch: { type: "string" },
  },
});
const root = resolve(opt.root);
const SAMPLE = Number(opt.sample);
const scratch = opt.scratch ?? join(root, ".ts71-scratch");

// ---- RPC accounting ------------------------------------------------------------------------
let task = "init";
const rpc = new Map(); // `${task}\t${method}` -> {calls, total_ms, samples}
const record = (method, ms) => {
  const k = `${task}\t${method}`;
  let r = rpc.get(k);
  if (!r) rpc.set(k, (r = { task, method, calls: 0, total_ms: 0, samples: [] }));
  r.calls++; r.total_ms += ms; r.samples.push(ms);
};
const C = client_mod.Client.prototype;
for (const name of ["apiRequest", "apiRequestBinary"]) {
  const orig = C[name];
  C[name] = function (method, params) {
    const t = performance.now();
    try { return orig.call(this, method, params); } finally { record(method, performance.now() - t); }
  };
}
const orig_batch = C.batchRequests;
C.batchRequests = function (requests) {
  const t = performance.now();
  try { return orig_batch.call(this, requests); }
  finally {
    const ms = performance.now() - t;
    record("batchRequests", ms);
    for (const r of requests) record(`batched:${r.method}`, ms / requests.length);
  }
};

// ---- task runner ---------------------------------------------------------------------------
// mem[0] server pid, mem[1] server peak KB, mem[2] client peak KB, mem[3] stop; a worker samples while RPCs block main.
const mem = new Int32Array(new SharedArrayBuffer(16));
const SAMPLER = `const { workerData } = require("node:worker_threads"); const { execFileSync } = require("node:child_process");
const a = new Int32Array(workerData);
const up = (i, v) => { let o; while ((o = Atomics.load(a, i)) < v && Atomics.compareExchange(a, i, o, v) !== o); };
while (!Atomics.load(a, 3)) {
  up(2, Math.round(process.memoryUsage().rss / 1024));
  const pid = Atomics.load(a, 0);
  if (pid) { try { up(1, Number(execFileSync("ps", ["-o", "rss=", "-p", String(pid)]).toString().trim()) || 0); } catch {} }
  Atomics.wait(a, 3, 0, 25);
}`;
const sampler = new Worker(SAMPLER, { eval: true, workerData: mem.buffer });
const server_rss_kb = () => {
  if (!mem[0]) return 0;
  try { return Number(execFileSync("ps", ["-o", "rss=", "-p", String(mem[0])]).toString().trim()) || 0; } catch { return 0; }
};
const client_rss_kb = () => Math.round(process.memoryUsage().rss / 1024);
const tasks = [];
const run = (name, inputs, fn) => {
  task = name;
  process.stderr.write(`task ${name}\n`);
  const list = inputs ?? [undefined];
  let ok = 0, errors = 0, first_error;
  Atomics.store(mem, 1, server_rss_kb()); Atomics.store(mem, 2, client_rss_kb());
  const t = performance.now();
  for (const x of list) {
    try { fn(x); ok++; } catch (e) { errors++; first_error ??= String(e?.message ?? e).slice(0, 300); }
  }
  const ms = performance.now() - t;
  const server_end = server_rss_kb(), client_end = client_rss_kb();
  tasks.push({ task: name, inputs: list.length, ok, errors, wall_ms: ms, first_error,
    server_peak_rss_mb: Math.max(Atomics.load(mem, 1), server_end) / 1024, client_peak_rss_mb: Math.max(Atomics.load(mem, 2), client_end) / 1024,
    server_end_rss_mb: server_end / 1024, client_end_rss_mb: client_end / 1024 });
  task = "idle";
  return ms;
};
const stride = (arr, n = SAMPLE) => {
  if (arr.length <= n) return arr;
  const step = arr.length / n, out = [];
  for (let i = 0; i < n; i++) out.push(arr[Math.floor(i * step)]);
  return out;
};
const counts = {};

// ---- setup ---------------------------------------------------------------------------------
let api;
run("init_spawn", null, () => { api = new API({ cwd: root }); api.ensureInitialized(); });
mem[0] = Number(execFileSync("pgrep", ["-P", String(process.pid)]).toString().trim().split("\n").find(p => {
  try { return /tsc/.test(execFileSync("ps", ["-o", "command=", "-p", p]).toString()); } catch { return false; }
}));
let snap;
run("snapshot_open_projects", null, () => {
  snap = api.createSnapshot({ openProjects: tsconfigs.map(p => resolve(root, p)) });
});
const projects = snap.getProjects();
counts.projects = projects.length;

// file -> owning project (first project listing it); corpus files only
const owner = new Map();
run("program_source_file_names", projects, p => {
  for (const f of p.program.getSourceFileNames()) {
    if (!f.startsWith(root + "/") || f.includes("/node_modules/") || f.endsWith(".d.ts") && f.includes("/lib.")) continue;
    if (!/\.(m|c)?tsx?$/.test(f)) continue;
    if (!owner.has(f)) owner.set(f, p);
  }
});
const files = [...owner.keys()].sort();
counts.files = files.length;
if (opt["files-out"]) writeFileSync(opt["files-out"], files.join("\n") + "\n");

// ---- bulk: AST ----------------------------------------------------------------------------
// Seeded reservoirs bound memory on large corpora; only the sampled files keep their SourceFile.
let seed = 0x7f4a7c15;
const rand = () => { seed = (seed + 0x6d2b79f5) | 0; let t = Math.imul(seed ^ (seed >>> 15), 1 | seed); t ^= t + Math.imul(t ^ (t >>> 7), 61 | t); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
const reservoir = () => {
  const r = { items: [], seen: 0 };
  r.push = x => {
    const i = r.seen++;
    if (r.items.length < SAMPLE) r.items.push([i, x]);
    else { const j = Math.floor(rand() * r.seen); if (j < SAMPLE) r.items[j] = [i, x]; }
  };
  r.list = () => r.items.sort((a, b) => a[0] - b[0]).map(([, x]) => x);
  return r;
};
const sfiles = stride(files);
const sfiles_set = new Set(sfiles);
const sfs = new Map();
const K = SyntaxKind;
const per_file_calls = new Map(), per_file_idents = new Map();
const pick = Object.fromEntries(["ident", "call", "arg", "type_ref", "func_decl", "decl", "module_spec", "export_spec",
  "shorthand", "enum_member", "prop_access"].map(k => [k, reservoir()]));
let node_count = 0, fetch_ms = 0, walk_ms = 0;
const walk = (f, sf) => {
  const calls = [], idents = [];
  const visit = n => {
    node_count++;
    switch (n.kind) {
      case K.Identifier: idents.push(n.getStart(sf)); pick.ident.push([f, n]); break;
      case K.CallExpression: {
        const e = n.expression;
        const name = e.kind === K.PropertyAccessExpression ? e.name : e.kind === K.Identifier ? e : undefined;
        if (name) calls.push(name.getStart(sf));
        pick.call.push([f, n]);
        if (n.arguments?.length) pick.arg.push([f, n.arguments[0]]);
        break;
      }
      case K.NewExpression: {
        const e = n.expression;
        const name = e.kind === K.PropertyAccessExpression ? e.name : e.kind === K.Identifier ? e : undefined;
        if (name) calls.push(name.getStart(sf));
        break;
      }
      case K.TypeReference: pick.type_ref.push([f, n]); break;
      case K.FunctionDeclaration: case K.MethodDeclaration: pick.func_decl.push([f, n]); pick.decl.push([f, n]); break;
      case K.ClassDeclaration: case K.InterfaceDeclaration: case K.VariableDeclaration: case K.TypeAliasDeclaration:
        pick.decl.push([f, n]); break;
      case K.ImportDeclaration: if (n.moduleSpecifier?.kind === K.StringLiteral) pick.module_spec.push([f, n.moduleSpecifier]); break;
      case K.ExportSpecifier: pick.export_spec.push([f, n]); break;
      case K.ShorthandPropertyAssignment: pick.shorthand.push([f, n]); break;
      case K.EnumMember: pick.enum_member.push([f, n]); break;
      case K.PropertyAccessExpression: pick.prop_access.push([f, n]); break;
    }
    n.forEachChild(visit);
  };
  sf.forEachChild(visit);
  per_file_calls.set(f, calls); per_file_idents.set(f, idents);
};
run("bulk_ast_fetch_and_walk", files, f => {
  const t0 = performance.now();
  const sf = owner.get(f).program.getSourceFile(f);
  const t1 = performance.now();
  walk(f, sf);
  fetch_ms += t1 - t0; walk_ms += performance.now() - t1;
  if (sfiles_set.has(f)) sfs.set(f, sf);
});
{
  const fw = tasks.at(-1);
  const peaks = { server_peak_rss_mb: fw.server_peak_rss_mb, client_peak_rss_mb: fw.client_peak_rss_mb, server_end_rss_mb: fw.server_end_rss_mb, client_end_rss_mb: fw.client_end_rss_mb };
  tasks.push({ task: "bulk_ast_fetch", inputs: files.length, ok: files.length, errors: 0, wall_ms: fetch_ms, ...peaks });
  tasks.push({ task: "bulk_ast_walk", inputs: files.length, ok: files.length, errors: 0, wall_ms: walk_ms, ...peaks });
}
counts.ast_nodes = node_count;
counts.call_sites = [...per_file_calls.values()].reduce((a, b) => a + b.length, 0);
counts.identifiers = [...per_file_idents.values()].reduce((a, b) => a + b.length, 0);

// ---- bulk: semantic, comparable to ryi --resolve ------------------------------------------
const lroot = root.toLowerCase() + "/";
const in_corpus = s => s?.declarations?.some(d => { const p = String(d.path).toLowerCase(); return p.startsWith(lroot) && !p.includes("/node_modules/"); });
const call_sym_sample = reservoir(), alias_syms = [];
let resolved_in_corpus = 0, unresolved = 0, found = 0;
run("bulk_call_symbols", files, f => {
  const pos = per_file_calls.get(f);
  if (!pos.length) return;
  for (const s of owner.get(f).checker.getSymbolAtPosition(f, pos)) {
    if (!s) { unresolved++; continue; }
    found++;
    call_sym_sample.push([f, s]);
    if (s.flags & SymbolFlags.Alias) alias_syms.push([f, s]);
    else if (in_corpus(s)) resolved_in_corpus++;
  }
});
const target_sample = reservoir();
run("bulk_call_alias_targets", alias_syms, ([f, s]) => {
  const t = owner.get(f).checker.getAliasedSymbol(s);
  target_sample.push([f, t]);
  if (in_corpus(t)) resolved_in_corpus++;
});
const call_syms = call_sym_sample.list(), targets = target_sample.list();
counts.call_symbols = found;

counts.call_unresolved = unresolved;
counts.call_alias = alias_syms.length;
counts.call_decl_in_corpus = resolved_in_corpus;

run("bulk_call_types", files, f => {
  const pos = per_file_calls.get(f);
  if (pos.length) owner.get(f).checker.getTypeAtPosition(f, pos);
});
run("bulk_identifier_symbols", files, f => {
  const pos = per_file_idents.get(f);
  if (pos.length) owner.get(f).checker.getSymbolAtPosition(f, pos);
});
const by_project = new Map();
for (const f of files) { const p = owner.get(f); (by_project.get(p) ?? by_project.set(p, []).get(p)).push(f); }
let sem_diags = 0;
run("bulk_semantic_diagnostics", [...by_project], ([p, fs]) => { sem_diags += p.program.getSemanticDiagnostics(fs).length; });
counts.semantic_diagnostics = sem_diags;

// ---- per-method sample --------------------------------------------------------------------
const proj0 = projects.find(p => by_project.has(p)) ?? projects[0];
const ck = f => owner.get(f).checker;
const S = Object.fromEntries(Object.entries(pick).map(([k, v]) => [k, v.list()]));
const syms = stride(call_syms.filter(([, s]) => s));
const aliases = stride(alias_syms);

// types and signatures gathered by tasks below
const types = [], sigs = [];
const keep = (arr, f, x) => { if (x) arr.push([f, x]); };

run("getSymbolAtLocation", S.ident, ([f, n]) => ck(f).getSymbolAtLocation(n));
run("getSymbolsAtLocations", sfiles, f => {
  const ns = S.ident.filter(([g]) => g === f).map(([, n]) => n);
  if (ns.length) ck(f).getSymbolAtLocation(ns);
});
run("getSymbolAtPosition", S.ident, ([f, n]) => ck(f).getSymbolAtPosition(f, n.getStart()));
run("getTypeAtLocation", S.ident, ([f, n]) => keep(types, f, ck(f).getTypeAtLocation(n)));
run("getTypeAtLocations", sfiles, f => {
  const ns = S.ident.filter(([g]) => g === f).map(([, n]) => n);
  if (ns.length) ck(f).getTypeAtLocation(ns);
});
run("getTypeAtPosition", S.ident, ([f, n]) => ck(f).getTypeAtPosition(f, n.getStart()));
run("getResolvedSymbol", S.ident, ([f, n]) => ck(f).getResolvedSymbol(n));
run("getSymbolOfNode", S.decl, ([f, n]) => ck(f).getSymbolOfNode(n));
run("getSymbolOfDeclaration", S.decl, ([f, n]) => ck(f).getSymbolOfDeclaration(n));
run("api.getSymbol", S.decl, ([, n]) => api.getSymbol(n));
run("getShorthandAssignmentValueSymbol", S.shorthand, ([f, n]) => ck(f).getShorthandAssignmentValueSymbol(n));
run("getExportSpecifierLocalTargetSymbol", S.export_spec, ([f, n]) => ck(f).getExportSpecifierLocalTargetSymbol(n));
run("getConstantValue", S.enum_member.concat(S.prop_access.slice(0, 20)), ([f, n]) => ck(f).getConstantValue(n));
run("isContextSensitive", S.arg, ([f, n]) => ck(f).isContextSensitive(n));
run("getContextualType", S.arg, ([f, n]) => keep(types, f, ck(f).getContextualType(n)));
run("getContextualTypeForArgumentAtIndex", S.call.filter(([, n]) => n.arguments?.length), ([f, n]) => ck(f).getContextualTypeForArgumentAtIndex(n, 0));
run("getResolvedSignature", S.call, ([f, n]) => keep(sigs, f, ck(f).getResolvedSignature(n)));
run("getSignatureFromDeclaration", S.func_decl, ([f, n]) => keep(sigs, f, ck(f).getSignatureFromDeclaration(n)));
run("getTypeFromTypeNode", S.type_ref, ([f, n]) => keep(types, f, ck(f).getTypeFromTypeNode(n)));
run("resolveName", S.ident.slice(0, 50), ([f, n]) => ck(f).resolveName(n.text, SymbolFlags.Value, n, false));
run("getSymbolsInScope", S.ident.slice(0, 20), ([f, n]) => ck(f).getSymbolsInScope(n, SymbolFlags.Value));

// symbols
run("getTypeOfSymbol", syms, ([f, s]) => keep(types, f, ck(f).getTypeOfSymbol(s)));
run("getTypesOfSymbols", sfiles, f => {
  const ss = syms.filter(([g]) => g === f).map(([, s]) => s);
  if (ss.length) ck(f).getTypeOfSymbol(ss);
});
run("getDeclaredTypeOfSymbol", syms, ([f, s]) => keep(types, f, ck(f).getDeclaredTypeOfSymbol(s)));
run("getNonMissingTypeOfSymbol", syms, ([f, s]) => ck(f).getNonMissingTypeOfSymbol(s));
run("getTypeOfSymbolAtLocation", S.ident.slice(0, SAMPLE), ([f, n]) => {
  const s = ck(f).getSymbolAtLocation(n);
  if (s) ck(f).getTypeOfSymbolAtLocation(s, n);
});
run("getParentOfSymbol", syms, ([f, s]) => ck(f).getParentOfSymbol(s));
run("symbol.getParent", syms, ([, s]) => s.getParent());
run("getMergedSymbol", syms, ([f, s]) => ck(f).getMergedSymbol(s));
run("getAliasedSymbol", aliases, ([f, s]) => ck(f).getAliasedSymbol(s));
run("getImmediateAliasedSymbol", aliases, ([f, s]) => ck(f).getImmediateAliasedSymbol(s));
run("getTargetSymbol", syms, ([f, s]) => ck(f).getTargetSymbol(s));
run("getExportSymbolOfSymbol", syms, ([f, s]) => ck(f).getExportSymbolOfSymbol(s));
run("symbol.getExportSymbol", syms, ([, s]) => s.getExportSymbol());
run("getFullyQualifiedName", syms, ([f, s]) => ck(f).getFullyQualifiedName(s));
run("getJsDocTagsOfSymbol", syms, ([f, s]) => ck(f).getJsDocTagsOfSymbol(s));
run("getDocumentationCommentOfSymbol", syms, ([f, s]) => ck(f).getDocumentationCommentOfSymbol(s));
run("isReadonlySymbol", syms, ([f, s]) => ck(f).isReadonlySymbol(s));
run("isUnknownSymbol", syms, ([f, s]) => ck(f).isUnknownSymbol(s));
run("isUndefinedSymbol", syms, ([f, s]) => ck(f).isUndefinedSymbol(s));
run("isArgumentsSymbol", syms, ([f, s]) => ck(f).isArgumentsSymbol(s));
run("symbol.getMembers", syms, ([, s]) => s.getMembers());
run("symbol.getExports", syms, ([, s]) => s.getExports());
run("getSymbolOfSourceFile", sfiles, f => ck(f).getSymbolOfSourceFile(f));
run("getSymbolsOfSourceFiles", [...by_project], ([p, fs]) => p.checker.getSymbolOfSourceFile(stride(fs)));
const mods = [];
run("getExportsOfModule", sfiles, f => {
  const m = ck(f).getSymbolOfSourceFile(f);
  if (m) { const ex = ck(f).getExportsOfModule(m); if (ex.length) mods.push([f, m, ex[0].name]); }
});
run("getMemberInModuleExports", mods, ([f, m, name]) => ck(f).getMemberInModuleExports(m, name));
run("getReferencesToSymbolInFile", syms.slice(0, 50), ([f, s]) => ck(f).getReferencesToSymbolInFile(f, s));

// types
const ts_ = stride(types);
const tp = ts_.filter(([, t]) => t.flags & TypeFlags.TypeParameter);
const obj = ts_.filter(([, t]) => t.flags & TypeFlags.Object);
const iface = obj.filter(([, t]) => t.objectFlags & (ObjectFlags.Class | ObjectFlags.Interface));
const tref = obj.filter(([, t]) => t.objectFlags & ObjectFlags.Reference);
const union = ts_.filter(([, t]) => t.flags & TypeFlags.Union);
const lit = ts_.filter(([, t]) => t.flags & (TypeFlags.StringLiteral | TypeFlags.NumberLiteral | TypeFlags.BooleanLiteral));
const cond = ts_.filter(([, t]) => t.flags & TypeFlags.Conditional);
const idxacc = ts_.filter(([, t]) => t.flags & TypeFlags.IndexedAccess);
const subst = ts_.filter(([, t]) => t.flags & TypeFlags.Substitution);
const mapped = obj.filter(([, t]) => t.objectFlags & ObjectFlags.Mapped);
counts.sample_types = ts_.length;
run("getSignaturesOfType", ts_, ([f, t]) => { for (const s of ck(f).getSignaturesOfType(t, SignatureKind.Call)) sigs.push([f, s]); });
run("type.getConstructSignatures", ts_, ([, t]) => t.getConstructSignatures());
run("getAwaitedType", ts_, ([f, t]) => ck(f).getAwaitedType(t));
run("getBaseTypeOfLiteralType", ts_, ([f, t]) => ck(f).getBaseTypeOfLiteralType(t));
run("getNonNullableType", ts_, ([f, t]) => ck(f).getNonNullableType(t));
run("getWidenedType", ts_, ([f, t]) => ck(f).getWidenedType(t));
run("isArrayLikeType", ts_, ([f, t]) => ck(f).isArrayLikeType(t));
run("isArrayType", ts_, ([f, t]) => ck(f).isArrayType(t));
run("isTupleType", ts_, ([f, t]) => ck(f).isTupleType(t));
run("isTypeAssignableTo", ts_, ([f, t]) => ck(f).isTypeAssignableTo(t, ts_[0][1]));
run("typeToString", ts_, ([f, t]) => ck(f).typeToString(t));
run("typeToTypeNode", ts_, ([f, t]) => ck(f).typeToTypeNode(t));
run("getApparentType", ts_, ([f, t]) => ck(f).getApparentType(t));
run("getReducedType", ts_, ([f, t]) => ck(f).getReducedType(t));
const with_props = [];
run("getPropertiesOfType", ts_, ([f, t]) => { const ps = ck(f).getPropertiesOfType(t); if (ps.length) with_props.push([f, t, ps[0].name]); });
run("type.getApparentProperties", ts_, ([, t]) => t.getApparentProperties());
run("getPropertyOfType", with_props, ([f, t, n]) => ck(f).getPropertyOfType(t, n));
run("getTypeOfPropertyOfType", with_props, ([f, t, n]) => ck(f).getTypeOfPropertyOfType(t, n));
run("getIndexInfosOfType", ts_, ([f, t]) => ck(f).getIndexInfosOfType(t));
run("getIndexInfoOfType", ts_, ([f, t]) => ck(f).getIndexInfoOfType(t, IndexKind.String));
run("getIndexTypeOfType", ts_, ([f, t]) => ck(f).getIndexTypeOfType(t, IndexKind.String));
run("getBaseConstraintOfType", ts_, ([f, t]) => ck(f).getBaseConstraintOfType(t));
run("type.getSymbol", ts_, ([, t]) => t.getSymbol());
run("type.getAliasSymbol", ts_, ([, t]) => t.getAliasSymbol());
run("type.getAliasTypeArguments", ts_, ([, t]) => t.getAliasTypeArguments());
run("getConstraintOfTypeParameter", tp, ([f, t]) => ck(f).getConstraintOfTypeParameter(t));
run("getDefaultFromTypeParameter", tp, ([f, t]) => ck(f).getDefaultFromTypeParameter(t));
run("getBaseTypes", iface, ([f, t]) => ck(f).getBaseTypes(t));
run("type.getTypeParameters", iface, ([, t]) => t.getTypeParameters());
run("type.getOuterTypeParameters", iface, ([, t]) => t.getOuterTypeParameters());
run("type.getLocalTypeParameters", iface, ([, t]) => t.getLocalTypeParameters());
run("type.getThisType", iface, ([, t]) => t.getThisType());
run("getTypeArguments", tref, ([f, t]) => ck(f).getTypeArguments(t));
run("type.getTarget", tref, ([, t]) => t.getTarget());
run("type.getTypes", union, ([, t]) => t.getTypes());
run("type.getFreshType", lit, ([, t]) => t.getFreshType());
run("type.getRegularType", lit, ([, t]) => t.getRegularType());
run("type.getCheckType", cond, ([, t]) => t.getCheckType());
run("type.getExtendsType", cond, ([, t]) => t.getExtendsType());
run("type.getTrueType", cond, ([, t]) => t.getTrueType());
run("type.getFalseType", cond, ([, t]) => t.getFalseType());
run("type.getObjectType", idxacc, ([, t]) => t.getObjectType());
run("type.getIndexType", idxacc, ([, t]) => t.getIndexType());
run("type.getBaseType", subst, ([, t]) => t.getBaseType());
run("type.getConstraint", subst.concat(tp), ([, t]) => t.getConstraint());
run("type.getTypeParameter", mapped, ([, t]) => t.getTypeParameter());
run("type.getConstraintType", mapped, ([, t]) => t.getConstraintType());
run("type.getNameType", mapped, ([, t]) => t.getNameType());
run("type.getTemplateType", mapped, ([, t]) => t.getTemplateType());
counts.sample_type_kinds = JSON.stringify({ tp: tp.length, iface: iface.length, tref: tref.length, union: union.length, lit: lit.length, cond: cond.length, idxacc: idxacc.length, subst: subst.length, mapped: mapped.length });

// signatures
const sg = stride(sigs);
counts.sample_signatures = sg.length;
run("getReturnTypeOfSignature", sg, ([f, s]) => ck(f).getReturnTypeOfSignature(s));
run("getRestTypeOfSignature", sg, ([f, s]) => ck(f).getRestTypeOfSignature(s));
run("getTypePredicateOfSignature", sg, ([f, s]) => ck(f).getTypePredicateOfSignature(s));
run("getParameterType", sg, ([f, s]) => ck(f).getParameterType(s, 0));
run("signatureToSignatureDeclaration", sg, ([f, s]) => ck(f).signatureToSignatureDeclaration(s, K.FunctionDeclaration));
run("isUnknownSignature", sg, ([f, s]) => ck(f).isUnknownSignature(s));
run("signature.getTypeParameters", sg, ([, s]) => s.getTypeParameters());
run("signature.getParameters", sg, ([, s]) => s.getParameters());
run("signature.getThisParameter", sg, ([, s]) => s.getThisParameter());
run("signature.getTarget", sg, ([, s]) => s.getTarget());
run("signature.getTypeParameterAtPosition", sg, ([, s]) => s.getTypeParameterAtPosition(0));

// intrinsic types
const c0 = proj0.checker;
for (const m of ["getAnyType", "getStringType", "getNumberType", "getBooleanType", "getVoidType", "getUndefinedType",
  "getNullType", "getNeverType", "getUnknownType", "getBigIntType", "getESSymbolType", "getNonPrimitiveType"]) {
  run(m, null, () => c0[m]());
}

// program
run("getDefaultProjectForFile", sfiles, f => snap.getDefaultProjectForFile(f));
run("getSourceFileMetadata", sfiles, f => owner.get(f).program.getSourceFileMetadata(f));
run("isSourceFileFromExternalLibrary", sfiles, f => owner.get(f).program.isSourceFileFromExternalLibrary(sfs.get(f)));
run("isSourceFileDefaultLibrary", sfiles, f => owner.get(f).program.isSourceFileDefaultLibrary(sfs.get(f)));
run("getResolvedModuleFromModuleSpecifier", S.module_spec, ([f, n]) => owner.get(f).program.getResolvedModuleFromModuleSpecifier(n, f));
run("getModeForUsageLocation", S.module_spec, ([f, n]) => owner.get(f).program.getModeForUsageLocation(f, n));
run("getResolvedModule", S.module_spec, ([f, n]) => owner.get(f).program.getResolvedModule(f, n.text, 0));
run("getModeForResolutionAtIndex", S.module_spec, ([f]) => owner.get(f).program.getModeForResolutionAtIndex(f, 0));
const trd = sfiles.flatMap(f => (sfs.get(f).typeReferenceDirectives ?? []).map(d => [f, d]));
run("getResolvedTypeReferenceDirectiveFromTypeReferenceDirective", trd, ([f, d]) => owner.get(f).program.getResolvedTypeReferenceDirectiveFromTypeReferenceDirective(d, f));
run("getResolvedTypeReferenceDirective", sfiles.slice(0, 20), f => owner.get(f).program.getResolvedTypeReferenceDirective(f, "node", 0));
run("getConfigFileNames", projects, p => p.program.getConfigFileNames());
run("getConfigSourceFile", projects.filter(p => p.configFileName), p => p.program.getConfigSourceFile(p.configFileName));
for (const m of ["getSyntacticDiagnostics", "getBindDiagnostics", "getSemanticDiagnostics", "getSuggestionDiagnostics", "getDeclarationDiagnostics"]) {
  run(m, sfiles, f => owner.get(f).program[m](f));
}
for (const m of ["getProgramDiagnostics", "getGlobalDiagnostics", "getConfigFileParsingDiagnostics"]) {
  run(m, projects, p => p.program[m]());
}
run("getJavaScriptEmit", sfiles, f => owner.get(f).program.getJavaScriptEmit([f]));
run("getDeclarationEmit", sfiles, f => owner.get(f).program.getDeclarationEmit([f]));
run("emitToString", [proj0], p => p.program.emitToString());

// language service
const ls = f => owner.get(f).languageService;
run("getCompletionsAtPosition", S.prop_access.slice(0, 50), ([f, n]) => ls(f).getCompletionsAtPosition(f, n.name.getStart()));
run("getReferencedSymbolsForNode", S.ident.slice(0, 20), ([f, n]) => ls(f).getReferencedSymbolsForNode(n, n.getStart()));
run("getSignatureUsage", S.func_decl.slice(0, 20), ([f, n]) => ls(f).getSignatureUsage(n));
run("getImportEditsForSymbols", stride(targets, 20), ([f, s]) => ls(f).getImportEditsForSymbols(f, [s]));
run("getImportAdderEdits", stride(targets, 20), ([f, s]) => ls(f).getImportAdderEdits(f, [{ kind: "importSymbol", symbol: s }]));
run("formatNodeForInsertion", S.decl.slice(0, 20), ([f, n]) => ls(f).formatNodeForInsertion(n, f, 0));

// printer, source files, transpile
run("printNode", S.decl.slice(0, 50), ([, n]) => api.printer.printNode(n));
run("printFile", sfiles.slice(0, 50), f => api.printer.printFile(sfs.get(f)));
run("retainSourceFile", sfiles.slice(0, 50), f => api.retainSourceFile(sfs.get(f)).dispose());
run("createSourceFile", sfiles.slice(0, 50), f => api.createSourceFile(f, sfs.get(f).text).dispose());
run("createSourceFileFromFile", sfiles.slice(0, 50), f => api.createSourceFileFromFile(f).dispose());
run("transpileModule", sfiles.slice(0, 50), f => api.transpileModule(sfs.get(f).text, { fileName: f }));
run("transpileModuleFromFile", sfiles.slice(0, 50), f => api.transpileModuleFromFile(f));
run("transpileDeclaration", sfiles.slice(0, 50), f => api.transpileDeclaration(sfs.get(f).text, { fileName: f }));
run("transpileDeclarationFromFile", sfiles.slice(0, 50), f => api.transpileDeclarationFromFile(f));

// config, module resolution
const cfgs = tsconfigs.map(p => resolve(root, p));
run("parseConfigFile", cfgs, c => api.parseConfigFile(c));
run("readConfigFile", cfgs, c => api.readConfigFile(c));
run("parseJsonConfigFileContent", cfgs, c => api.parseJsonConfigFileContent(JSON.parse(JSON.stringify(api.readConfigFile(c).config ?? {})), { configFileName: c }));
run("parseCommandLine", null, () => api.parseCommandLine(["--strict", "--target", "es2022", "a.ts"]));
let resolver;
run("createModuleResolver", null, () => { resolver = api.createModuleResolver(proj0.program.getCompilerOptions()); });
run("resolveModuleName", S.module_spec, ([f, n]) => resolver.resolveModuleName(n.text, dirname(f)));
run("releaseModuleResolver", null, () => resolver.dispose());

// snapshots, incremental edit
run("getWellKnownSymbols+Signatures", null, () => { c0.getWellKnownSymbols?.(); c0.getWellKnownSignatures?.(); });
run("updateSnapshot", null, () => snap.update({}).dispose());
const edit_files = sfiles.slice(0, 5);
run("runWithTemporaryFileUpdate+semanticDiagnostics", edit_files, f => {
  api.runWithTemporaryFileUpdate(snap, f, sfs.get(f).text + "\nexport const __ts71_probe: number = 1;\n", s2 => {
    s2.getDefaultProjectForFile(f).program.getSemanticDiagnostics(f);
  });
});
run("getCurrentLanguageServerSnapshot", null, () => api.getCurrentLanguageServerSnapshot());
run("api.batch(getSymbolAtPosition)", [0], () => {
  api.batch(...S.ident.map(([f, n]) => ck(f).getSymbolAtPosition.gen(f, n.getStart())));
});

// separate program, build orchestrator, debug
run("createProgram", [proj0], p => {
  const fs = by_project.get(p) ?? [];
  api.createProgram(fs, p.program.getCompilerOptions());
});
mkdirSync(scratch, { recursive: true });
run("debug.cpuProfile", null, () => { api.debug.startCPUProfile(scratch); proj0.program.getSemanticDiagnostics(files[0]); api.debug.stopCPUProfile(); });
run("debug.saveHeapProfile", null, () => api.debug.saveHeapProfile(scratch));
run("emit", [proj0], p => p.program.emit());
if (!opt["skip-build"]) {
  let orch;
  const bc = proj0.configFileName;
  run("createBuildOrchestrator", null, () => { orch = api.createBuildOrchestrator([bc], { cwd: root }); });
  if (orch?.build) {
    run("build", null, () => orch.build(bc));
    run("buildReferences", null, () => orch.buildReferences(bc));
    run("cleanBuild", null, () => orch.clean(bc));
    run("cleanReferences", null, () => orch.cleanReferences(bc));
    run("disposeBuildOrchestrator", null, () => orch.dispose());
  }
}
run("dispose_snapshot", null, () => snap.dispose());
run("close", null, () => api.close());
Atomics.store(mem, 3, 1); Atomics.notify(mem, 3);
await new Promise(r => sampler.once("exit", r));

// ---- output -------------------------------------------------------------------------------
const median = a => { const s = [...a].sort((x, y) => x - y); return s.length ? s[Math.floor(s.length / 2)] : null; };
counts.server_peak_rss_mb = Math.max(...tasks.map(t => t.server_peak_rss_mb ?? 0));
counts.client_peak_rss_mb = Math.max(...tasks.map(t => t.client_peak_rss_mb ?? 0));
const out = {
  ts_version, root, tsconfigs, counts, tasks,
  rpc: [...rpc.values()].map(r => ({ task: r.task, method: r.method, calls: r.calls, total_ms: r.total_ms, median_call_ms: median(r.samples) })),
  api_methods: [...readFileSync(join(pkg, "dist/api/proto.generated.d.ts"), "utf8")
    .split("export interface APIMethodInfo {")[1].split("\n}")[0].matchAll(/^    (\w+): APIMethod</gm)].map(m => m[1]),
};
writeFileSync(opt.out, JSON.stringify(out, null, 1));
console.log(JSON.stringify(counts));
