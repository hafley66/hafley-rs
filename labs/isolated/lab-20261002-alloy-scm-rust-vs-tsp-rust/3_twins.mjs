// Extract assertion identities with TypeScript's AST. Never derive candidate input from an oracle.
import ts from "typescript";
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, join, relative, resolve } from "node:path";
const here = dirname(new URL(import.meta.url).pathname);
const root = process.argv[2] ?? "/Users/chrishafley/projects/hafley-tsp/packages/rust/src";
const supported = /^(00_name-policy|components\/(0_primitives|1_declarations|2_references|3_files)\/)/;
const files = [];
function walk(dir) { for (const e of readdirSync(dir, { withFileTypes: true })) { const p=join(dir,e.name); if(e.isDirectory()) walk(p); else if(/\.test\.tsx?$/.test(p))files.push(p); } }
walk(root); files.sort();
const manifest = [];
function gapFor(file) {
  if (file.startsWith("symbols/")) return ["scopes/symbol tables", "The candidate has OutputSymbol declarations; no named-type/function symbol factories or reactive typeKind metadata"];
  if (file.includes("ReplaceFile")) return ["other", "The grammar printer has no existing-file zone splice or manual-content preservation API"];
  if (file.includes("CodegenPair")) return ["refkey/import resolution", "The candidate has no paired-file context, impl delegation, or module synthesis"];
  if (file.includes("adapter")) return ["other", "No TypeSpec compiler-to-neutral adapter or model/emitter pipeline in candidate B"];
  if (file.includes("04_ops-plan")) return ["other", "No operation planning model or TypeSpec decorator registry in candidate B"];
  if (file.includes("cargo-check")) return ["other", "The A assertion depends on a whole generated scratch crate and Cargo check; B has no crate emitter"];
  return ["other", "No endpoint/routing/model/daemon emitter pipeline in candidate B; grammar nodes accept explicit syntax props"];
}
for (const file of files) {
  const name=relative(root,file); const source=readFileSync(file,"utf8");
  const sf=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
  const assertions=[],tests=[];let current;
  function visit(n) {
    const prev=current;
    if(ts.isCallExpression(n)&& /^(it|test)(\.|$)/.test(n.expression.getText(sf)) && n.arguments.length>=2 && (ts.isArrowFunction(n.arguments[1])||ts.isFunctionExpression(n.arguments[1]))) {
      current={title:n.arguments[0].getText(sf),call:n,fn:n.arguments[1],ids:[]};tests.push(current);
    }
    if(ts.isCallExpression(n)&&ts.isPropertyAccessExpression(n.expression)&&/^to[A-Z]/.test(n.expression.name.text)) {
      let x=n.expression.expression;
      while(ts.isPropertyAccessExpression(x))x=x.expression;
      if(ts.isCallExpression(x)&&x.expression.getText(sf)==="expect") {
        const ordinal=assertions.length+1;
        const id=createHash("sha256").update(name+":"+ordinal).digest("hex").slice(0,16);
        const a={id,file:name,ordinal,line:sf.getLineAndCharacterOfPosition(n.getStart(sf)).line+1,matcher:n.expression.name.text,negated:n.expression.expression.getText(sf).endsWith(".not"),primary:["toBe","toMatchInlineSnapshot"].includes(n.expression.name.text),expectedLiteral:n.arguments[0]?.getText(sf)??"null",input:x.arguments[0].getText(sf),testTitle:current?.title??"top-level",testInput:current?.fn.body.getText(sf)??"",sourceHash:createHash("sha256").update(source).digest("hex"),node:n};
        assertions.push(a);current?.ids.push(id);
      }
    }
    ts.forEachChild(n,visit); current=prev;
  }
  visit(sf);
  manifest.push(...assertions.map(({node,...a})=>a));
  const out=join(here,"twins",name);mkdirSync(dirname(out),{recursive:true});
  const fixture=join(here,"fixtures",name+".txt");mkdirSync(dirname(fixture),{recursive:true});writeFileSync(fixture,source);
  const harness=relative(dirname(out),join(here,"4_probe.js"));
  const bridge=relative(dirname(out),join(here,"2_components.js"));
  if(!supported.test(name)) {
    const [reason,detail]=gapFor(name);
    writeFileSync(out,`// B twin of ${name}; original inputs and oracle literals are in fixtures/ and 3_assertions.json.\nimport { it } from "vitest";\nimport { gap } from ${JSON.stringify(harness.startsWith('.')?harness:'./'+harness)};\n`+assertions.map(a=>`it(${JSON.stringify(a.testTitle+" assertion "+a.ordinal)}, () => gap(${JSON.stringify(a.id)}, ${JSON.stringify(reason)}, ${JSON.stringify(detail)}));`).join("\n")+"\n");
  } else {
    const edits=[]; let usesRender=false;
    for(const n of sf.statements) if(ts.isImportDeclaration(n)) {
      const from=n.moduleSpecifier.text;
      if(from === "@alloy-js/core" && n.importClause?.namedBindings && ts.isNamedImports(n.importClause.namedBindings)) {
        const names=n.importClause.namedBindings.elements;
        if(names.some(e=>e.name.text === "render")) {
          usesRender=true;
          edits.push([n.getStart(sf),n.end,`import { ${names.filter(e=>e.name.text !== "render").map(e=>e.getText(sf)).join(", ")} } from "@alloy-js/core";`]);
        }
      }
      if(from.startsWith("."))edits.push([n.moduleSpecifier.getStart(sf),n.moduleSpecifier.end,JSON.stringify(bridge.startsWith('.')?bridge:'./'+bridge)]);
    }
    for(const a of assertions) {
      const n=a.node;let x=n.expression.expression;while(ts.isPropertyAccessExpression(x))x=x.expression;
      edits.push([n.getStart(sf),n.end,`probe(${JSON.stringify(a.id)}, () => (${x.arguments[0].getText(sf)}), ${n.arguments[0]?.getText(sf)??"null"})`]);
    }
    // Insert a wrapper around each test body; guards assertions after a missing rendering capability.
    for(const t of tests) {
      edits.push([t.fn.body.getStart(sf),t.fn.body.getStart(sf),`probeTest(${JSON.stringify(t.ids)}, () => `]);
      edits.push([t.fn.body.end,t.fn.body.end,`)`]);
    }
    edits.sort((a,b)=>b[0]-a[0]);let text=source;for(const [s,e,v] of edits)text=text.slice(0,s)+v+text.slice(e);
    text=`import { probe, probeTest${usesRender?", render":""} } from ${JSON.stringify(harness.startsWith('.')?harness:'./'+harness)};\n`+text;
    writeFileSync(out,text);
  }
}
writeFileSync(join(here,"3_assertions.json"),JSON.stringify(manifest,null,2)+"\n");
console.log(`${files.length} twins; ${manifest.filter(a=>a.primary).length} primary assertions; ${manifest.length} total assertions`);
