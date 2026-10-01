// `node 0_gen.mjs <lang>`: grammar.json + node-types.json + locals.scm -> gen/<lang>/0_nodes.tsx,
// one alloy component per named node kind (all of them, or <lang>/0_subset.mjs SUBSET).
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const here = dirname(new URL(import.meta.url).pathname);
const worktree = resolve(here, "../../..");

// Grammar dirs (holding src/grammar.json) of every tree-sitter-* dependency of hafley_scm, keyed by
// grammar.json `name`. A crate with several grammars (typescript/, tsx/) yields one entry per subdir.
let cargoCache;
export function cargoGrammars() {
  if (cargoCache) return cargoCache;
  const meta = JSON.parse(execFileSync("cargo", ["metadata", "--format-version", "1"], { cwd: worktree, maxBuffer: 1 << 28, encoding: "utf8" }));
  const scm = meta.packages.find((p) => p.name === "hafley_scm");
  const deps = new Set(scm.dependencies.filter((d) => d.name.startsWith("tree-sitter-") && d.kind === null).map((d) => d.name));
  cargoCache = new Map();
  for (const pkg of meta.packages.filter((p) => deps.has(p.name))) {
    const crate = dirname(pkg.manifest_path);
    const dirs = existsSync(join(crate, "src/grammar.json"))
      ? [crate]
      : readdirSync(crate).map((d) => join(crate, d)).filter((d) => existsSync(join(d, "src/grammar.json")));
    for (const dir of dirs) {
      const { name } = JSON.parse(readFileSync(join(dir, "src/grammar.json"), "utf8"));
      cargoCache.set(name, { lang: name, from: `cargo ${pkg.name} ${pkg.version}`, crate, dir });
    }
  }
  return cargoCache;
}

// lang -> { dir, crate, from }: hafley_scm's cargo grammar crates first, else npm `tree-sitter-<lang>` (C).
export function grammarSource(lang) {
  const cargo = cargoGrammars().get(lang);
  if (cargo) return cargo;
  const req = createRequire(import.meta.url);
  const dir = dirname(req.resolve(`tree-sitter-${lang}/package.json`));
  return { lang, from: `npm tree-sitter-${lang} ${JSON.parse(readFileSync(join(dir, "package.json"), "utf8")).version}`, crate: dir, dir };
}

// full: every named kind, ignoring <lang>/0_subset.mjs (3_report.mjs)
export async function generate(lang, { full = false, outDir = join(here, "gen", lang) } = {}) {
  const src = grammarSource(lang);
  const grammar = JSON.parse(readFileSync(join(src.dir, "src/grammar.json"), "utf8"));
  const nodeTypes = JSON.parse(readFileSync(join(src.dir, "src/node-types.json"), "utf8"));
  const shippedLocals = [join(src.dir, "queries/locals.scm"), join(src.crate, "queries/locals.scm")].find(existsSync);
  const handLocals = join(here, lang, "locals.scm");
  const localsPath = shippedLocals ?? (existsSync(handLocals) ? handLocals : undefined);
  const localsScm = localsPath ? readFileSync(localsPath, "utf8") : "";
  const subsetPath = join(here, lang, "0_subset.mjs");
  const nodeInfo = new Map(nodeTypes.filter((n) => n.named && !n.subtypes).map((n) => [n.type, n]));
  const SUBSET = !full && existsSync(subsetPath) ? (await import(pathToFileURL(subsetPath).href)).SUBSET : [...nodeInfo.keys()];
  const inSubset = new Set(SUBSET);
  const rules = grammar.rules;
  const supertypes = new Set((grammar.supertypes ?? []).map((s) => (typeof s === "string" ? s : s.name)));
  const externals = new Set((grammar.externals ?? []).filter((e) => e.type === "SYMBOL").map((e) => e.name));

  // locals.scm: only the two shapes `(kind) @cap` and `(kind field: (leaf) @cap)`
  const scopes = new Set();
  const definitions = new Map(); // kind -> field holding the defined name
  const references = new Set();
  for (const m of localsScm.matchAll(/^\((\w+)(?:\s+(\w+):\s*\((\w+)\)\s*@local\.(\w+))?\)\s*(?:@local\.(\w+))?/gm)) {
    const [, kind, fieldName, , innerCap, outerCap] = m;
    if (outerCap === "scope") scopes.add(kind);
    if (outerCap === "reference") references.add(kind);
    if (innerCap === "definition") definitions.set(kind, fieldName);
  }

  const walk = (r, f) => { f(r); for (const c of [r.content, ...(r.members ?? [])]) if (c) walk(c, f); };
  let aliasCount = 0;
  const aliasSource = new Map(); // named alias value -> first aliased rule
  for (const r of Object.values(rules)) walk(r, (x) => {
    if (x.type !== "ALIAS") return;
    aliasCount++;
    if (x.named && !aliasSource.has(x.value)) aliasSource.set(x.value, x.content);
  });

  const PREC = new Set(["PREC", "PREC_LEFT", "PREC_RIGHT", "PREC_DYNAMIC", "RESERVED", "TOKEN", "IMMEDIATE_TOKEN"]);
  // visible kinds reachable from a rule without crossing a FIELD/STRING (choice-of-symbols)
  function reachableKinds(rule, seen = new Set()) {
    switch (rule.type) {
      case "SYMBOL":
        if (!rule.name.startsWith("_")) return [rule.name];
        if (seen.has(rule.name) || !rules[rule.name]) return [];
        seen.add(rule.name);
        return reachableKinds(rules[rule.name], seen);
      case "ALIAS": return rule.named ? [rule.value] : [];
      case "CHOICE": return rule.members.flatMap((m) => reachableKinds(m, seen));
      default: return PREC.has(rule.type) ? reachableKinds(rule.content, seen) : [];
    }
  }
  function isSymbolChoice(rule) {
    if (rule.type === "SYMBOL" || (rule.type === "ALIAS" && rule.named)) return true;
    if (rule.type === "CHOICE") return rule.members.every((m) => m.type === "BLANK" || isSymbolChoice(m));
    if (PREC.has(rule.type)) return isSymbolChoice(rule.content);
    return false;
  }
  const canBlank = (r) => r.type === "BLANK" || (r.type === "CHOICE" && r.members.some(canBlank)) || (PREC.has(r.type) && canBlank(r.content));

  // hidden rules are lowered once into H_<name>; null marks one on the current lowering path
  const hidden = new Map();
  let hiddenExternals = 0;
  function lowerHidden(name) {
    if (hidden.has(name)) return hidden.get(name) ?? "rec";
    hidden.set(name, null);
    const body = lower(rules[name]);
    hidden.set(name, body);
    return body;
  }
  const refTo = (name) => `ref(${JSON.stringify(name)}, () => H_${name})`;

  // grammar rule -> printer combinator source text. "never" = pruned (kind outside SUBSET).
  function lower(rule) {
    switch (rule.type) {
      case "STRING": return `lit(${JSON.stringify(rule.value)})`;
      case "BLANK": return "blank";
      case "PATTERN": return /^\\r\?\\n$|^\\n$/.test(rule.value) ? "nl" : "never";
      case "FIELD": return canBlank(rule.content) ? `opt(field(${JSON.stringify(rule.name)}))` : `field(${JSON.stringify(rule.name)})`;
      case "ALIAS":
        if (!rule.named) return `lit(${JSON.stringify(rule.value)})`;
        return inSubset.has(rule.value) ? "child" : "never";
      case "SYMBOL": {
        const name = rule.name;
        if (!name.startsWith("_")) return inSubset.has(name) ? "child" : "never";
        if (!rules[name]) { hiddenExternals++; return "blank"; } // hidden external token: zero-width here
        if (supertypes.has(name) || isSymbolChoice(rules[name])) {
          return reachableKinds(rule).some((k) => inSubset.has(k)) ? "child" : "never";
        }
        const body = lowerHidden(name);
        return body === "never" || body === "blank" ? body : refTo(name);
      }
      case "SEQ": {
        const xs = rule.members.map(lower).filter((x) => x !== "blank");
        if (xs.includes("never")) return "never";
        return xs.length === 0 ? "blank" : xs.length === 1 ? xs[0] : `seq(${xs.join(", ")})`;
      }
      case "CHOICE": {
        const strs = rule.members.filter((m) => m.type === "STRING").map((m) => m.value);
        // only strings (+ BLANK): anonymous keywords, consumed from props.keywords
        if (strs.length && rule.members.every((m) => m.type === "STRING" || m.type === "BLANK")) {
          const k = `kw(${JSON.stringify(strs)})`;
          return strs.length < rule.members.length ? `opt(${k})` : k;
        }
        const lowered = rule.members.map(lower);
        const hasBlank = lowered.includes("blank");
        const xs = [...new Set(lowered.filter((x) => x !== "never" && x !== "blank"))];
        if (xs.length === 0) return hasBlank ? "blank" : "never";
        const body = xs.length === 1 ? xs[0] : `choice(${xs.join(", ")})`;
        return hasBlank ? `opt(${body})` : body;
      }
      case "REPEAT": case "REPEAT1": {
        const x = lower(rule.content);
        if (x === "never" || x === "blank") return rule.type === "REPEAT" ? "blank" : x;
        return `rep(${x}, ${rule.type === "REPEAT1" ? 1 : 0})`;
      }
      default:
        if (PREC.has(rule.type)) return lower(rule.content);
        throw new Error(`unhandled rule type ${rule.type}`);
    }
  }

  // rule text plus the bodies of every H_ it references, transitively
  function closure(text, seen = new Set()) {
    let all = text;
    for (const [, name] of text.matchAll(/ref\("(\w+)"/g)) {
      if (seen.has(name)) continue;
      seen.add(name);
      all += " " + closure(hidden.get(name) ?? "never", seen);
    }
    return all;
  }

  const pascal = (k) => k.split("_").filter(Boolean).map((s) => s[0].toUpperCase() + s.slice(1)).join("");
  const isLeaf = (n) => !n.fields && !n.children || (n.fields && Object.keys(n.fields).length === 0 && !n.children);
  const ruleOf = (kind) => rules[kind] ?? aliasSource.get(kind);
  const rel = (p) => relative(outDir, join(here, p));
  const handPrint = existsSync(join(here, lang, "2_print.tsx"));
  const out = [
    `// GENERATED by 0_gen.mjs from ${src.from} grammar.json + node-types.json${localsPath ? " + locals.scm" : ""}.`,
    `// Do not edit; rerun \`node 0_gen.mjs ${lang}\`.`,
    `import type { Children as $Children, Refkey as $Refkey } from "@alloy-js/core";`,
    `import { Leaf as $Leaf, Node as $Node } from "${rel(handPrint ? `${lang}/2_print.js` : "core/1_plain.js")}";`,
    "IMPORTS",
    "",
  ];
  const names = new Set();
  let leaves = 0, pruned = 0;
  for (const kind of SUBSET) {
    const info = nodeInfo.get(kind);
    if (!info) throw new Error(`${lang}: ${kind} is not a named kind in node-types.json`);
    let C = pascal(kind) || "Kind";
    while (names.has(C)) C += "_";
    names.add(C);
    const rule = ruleOf(kind);
    if (isLeaf(info) || !rule || externals.has(kind)) {
      leaves++;
      out.push(`export interface ${C}Props { children: $Children }`);
      out.push(`export function ${C}(props: ${C}Props) {`);
      out.push(`  return <$Leaf kind="${kind}">{props.children}</$Leaf>;`);
      out.push("}", "");
      continue;
    }
    const lowered = lower(rule);
    if (lowered === "never") pruned++;
    const all = closure(lowered);
    const fields = info.fields ?? {};
    const def = definitions.get(kind);
    const props = Object.entries(fields)
      .filter(([f]) => all.includes(`field(${JSON.stringify(f)})`))
      .map(([f, s]) => `  ${/^[A-Za-z_$][\w$]*$/.test(f) ? f : JSON.stringify(f)}${s.required ? "" : "?"}: ${s.multiple ? "$Children | $Children[]" : "$Children"};`);
    if (/\bchild\b/.test(all)) props.push("  children?: $Children;");
    if (all.includes("kw(")) props.push("  keywords?: string[];");
    if (def) props.push("  refkey?: $Refkey;");
    const multi = Object.entries(fields).filter(([, s]) => s.multiple).map(([f]) => f);
    out.push(`const R_${kind}: Rule = ${lowered};`);
    out.push(`export interface ${C}Props {`, ...props, "}");
    out.push(`export function ${C}(props: ${C}Props) {`);
    out.push(`  return <$Node kind="${kind}" props={props} rule={R_${kind}} multi={${JSON.stringify(multi)}}` +
      `${def ? ` def="${def}"` : ""}${scopes.has(kind) ? " scope" : ""} />;`);
    out.push("}", "");
  }
  const referenced = new Set([...closure(out.join("\n")).matchAll(/ref\("(\w+)"/g)].map((m) => m[1]));
  const hiddenDefs = [...referenced].sort().map((n) => `const H_${n}: Rule = ${hidden.get(n) ?? "never"};`);
  const body = out.join("\n").replace("IMPORTS", [...hiddenDefs.length ? ["", ...hiddenDefs] : []].join("\n"));
  const used = ["blank", "child", "choice", "field", "kw", "lit", "never", "nl", "opt", "ref", "rep", "seq"]
    .filter((n) => new RegExp(`[=(, ]${n}\\b`).test(body));
  const file = body.replace(/^(import \{ Leaf.*)$/m, `$1\nimport { ${["type Rule", ...used].join(", ")} } from "${rel("core/0_print.js")}";`);
  mkdirSync(outDir, { recursive: true });
  writeFileSync(join(outDir, "0_nodes.tsx"), file);
  const ruleNames = Object.keys(rules);
  return {
    lang, from: src.from, components: SUBSET.length, leaves, pruned, hiddenRefs: referenced.size,
    hiddenRules: ruleNames.filter((n) => n.startsWith("_")).length, rules: ruleNames.length, aliases: aliasCount,
    externals: (grammar.externals ?? []).length, hiddenExternals, localsShipped: !!shippedLocals,
    locals: localsPath ? (shippedLocals ? "shipped" : "hand") : "none",
    scopes: [...scopes], defs: [...definitions.keys()], refs: [...references], lines: file.split("\n").length, path: join(outDir, "0_nodes.tsx"),
  };
}

if (process.argv[1] && resolve(process.argv[1]) === new URL(import.meta.url).pathname) {
  const langs = process.argv.slice(2);
  for (const lang of langs.length ? langs : ["c", "typespec"]) {
    const s = await generate(lang);
    console.log(`wrote gen/${lang}/0_nodes.tsx: ${s.components} kinds (${s.from}); scopes=${s.scopes} defs=${s.defs} refs=${s.refs}`);
  }
}
