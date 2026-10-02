// Join measurements to the immutable assertion inventory, validate every emitted Rust string,
// and write the per-file and per-assertion report. Formatting and byte equality are independent.
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
const here=dirname(new URL(import.meta.url).pathname);
const resultDir=join(here,".results");
if(process.argv.includes("--clean")){rmSync(resultDir,{recursive:true,force:true});process.exit(0);}
const manifest=JSON.parse(readFileSync(join(here,"3_assertions.json"),"utf8"));
if(manifest.length!==239||manifest.filter(a=>a.primary).length!==137)throw new Error("Reference assertion inventory changed");
const allowed=new Set(["refkey/import resolution","name policy","scopes/symbol tables","doc comments","attributes/derives","generics","whitespace/blank-line policy","other"]);
const rows=manifest.map(a=>{
  const r=JSON.parse(readFileSync(join(resultDir,a.id+".json"),"utf8"));
  if(r.id!==a.id||!["PASS","DIFF","GAP"].includes(r.status))throw new Error(`Invalid outcome ${a.id}`);
  const source=readFileSync(join(here,"fixtures",a.file+".txt"),"utf8");
  if(createHash("sha256").update(source).digest("hex")!==a.sourceHash)throw new Error(`Reference fixture changed: ${a.file}`);
  if(r.status==="GAP"&&!allowed.has(r.reason))throw new Error(`Unknown GAP reason ${r.reason}`);
  if(r.status==="DIFF"&&a.file.includes("1_declarations")) {
    if(a.input.includes("DocComment"))r.reason="doc comments";
    else if(a.input.includes("typeParams")||a.input.includes("lifetimes")||a.input.includes("ResultType"))r.reason="generics";
    else if(/TupleStruct|StaticDeclaration/.test(a.input))r.reason="other";
  }
  const textEquality=typeof r.actual==="string"&&typeof r.expected==="string";
  return {...a,...r,byteEqual:textEquality?r.actual===r.expected:null,whitespaceEqual:textEquality?r.actual.replace(/\s/g,"")===r.expected.replace(/\s/g,""):null};
});
const results=readdirSync(resultDir).filter(n=>n.endsWith('.json'));
if(results.length!==rows.length)throw new Error(`Unexpected result count ${results.length}`);
function rustText(a,text){
  if(a.matcher==="toBeNull"||typeof text!=="string")return null;
  if(a.file==="00_name-policy.test.ts") {
    const element=a.input.match(/,\s*"([^"]+)"\)/)?.[1];
    if(element==="lifetime")return `type Probe = &'${text} ();\n`;
    if(["struct","enum","type"].includes(element))return `struct ${text};\n`;
    if(element==="module")return `mod ${text} {}\n`;
    if(element==="constant")return `const ${text}: usize = 0;\n`;
    if(["field","variable","parameter"].includes(element))return `fn probe() { let ${text} = (); }\n`;
    return `fn ${text}() {}\n`;
  }
  if(a.input.startsWith("serde"))return `#[${text}]\nstruct Probe;\n`;
  if(a.input.includes("<LetDeclaration"))return `fn probe() {\n${text}\n}\n`;
  if(/<(Ref|BoxType|RcType|ArcType|OptionType|VecType|ResultType)(\s|>)/.test(a.input))return `type Probe = ${text};\n`;
  if(a.input.includes("<DocComment"))return `${text}\nstruct Probe;\n`;
  return text.endsWith("\n")?text:text+"\n";
}
mkdirSync(join(resultDir,"syntax"),{recursive:true});
function check(a,text,label){
  const rust=rustText(a,text);if(rust===null)return null;
  const path=join(resultDir,"syntax",a.id+"-"+label+".rs");writeFileSync(path,rust);
  const query=spawnSync("ryii",["query","--lang","rust","--query","(ERROR) @error (MISSING) @missing",path],{encoding:"utf8"});
  if(query.error||query.status!==0)throw new Error(`ryii unavailable or failed: ${query.error??query.stderr}`);
  const hits=query.stdout.trim()?query.stdout.trim().split("\n").map(l=>JSON.parse(l)):[];
  const errors=hits.filter(h=>"error" in h).length;
  const missing=hits.filter(h=>"missing" in h).length;
  if(errors+missing!==hits.length)throw new Error(`Unexpected ryii capture shape: ${query.stdout}`);
  const fmt=spawnSync("rustfmt",["--check","--edition","2021","--config","skip_children=true",path],{encoding:"utf8"});
  if(fmt.error||fmt.status===null)throw new Error(`rustfmt unavailable: ${fmt.error}`);
  return {context:rust,ERROR:errors,MISSING:missing,parseClean:hits.length===0,rustfmtExit:fmt.status,rustfmt:fmt.stdout+fmt.stderr};
}
for(const a of rows)if(a.status!=="GAP"){
  a.syntax=check(a,a.actual,"B");
  a.oracleSyntax=typeof a.expected==="string"?check(a,a.expected,"A"):null;
}
const fileDir=join(resultDir,"files");
const fileOutputs=existsSync(fileDir)?readdirSync(fileDir).sort().map((name,index)=>{
  const file=JSON.parse(readFileSync(join(fileDir,name),"utf8"));
  const test=rows.find(a=>a.id===file.testId);if(!test)throw new Error("Unknown rendered-file test "+file.testId);
  return {...file,referenceFile:test.file,referenceLine:test.line,syntax:check({id:"file-"+index,file:"rendered/"+file.path,matcher:"file",input:""},file.text,"B")};
}):[];
const fileSyntaxTotals={outputs:fileOutputs.length,parseClean:fileOutputs.filter(f=>f.syntax.parseClean).length,rustfmtClean:fileOutputs.filter(f=>f.syntax.rustfmtExit===0).length};
function tally(xs){return {assertions:xs.length,PASS:xs.filter(a=>a.status==="PASS").length,DIFF:xs.filter(a=>a.status==="DIFF").length,GAP:xs.filter(a=>a.status==="GAP").length};}
const primary=tally(rows.filter(a=>a.primary)),all=tally(rows);
const whitespaceRows=rows.map(a=>({...a,status:a.status==="DIFF"&&a.whitespaceEqual?"PASS":a.status}));
const whitespace={primary:tally(whitespaceRows.filter(a=>a.primary)),all:tally(whitespaceRows)};
const syntax=rows.filter(a=>a.syntax).map(a=>a.syntax);
const syntaxTotals={outputs:syntax.length,parseClean:syntax.filter(s=>s.parseClean).length,parseFailed:syntax.filter(s=>!s.parseClean).length,rustfmtClean:syntax.filter(s=>s.rustfmtExit===0).length,rustfmtFailed:syntax.filter(s=>s.rustfmtExit!==0).length};
const files=[...new Set(rows.map(a=>a.file))];
const perFile=files.map(file=>{
  const xs=rows.filter(a=>a.file===file),gaps={};for(const a of xs.filter(a=>a.status==="GAP"))gaps[a.reason]=(gaps[a.reason]??0)+1;
  return {file,...tally(xs),primary:tally(xs.filter(a=>a.primary)),topGap:Object.entries(gaps).sort((a,b)=>b[1]-a[1])[0]?.[0]??"none"};
});
const report={primary,all,whitespace,syntax:syntaxTotals,fileSyntax:fileSyntaxTotals,perFile,assertions:rows,renderedFiles:fileOutputs};
writeFileSync(join(here,"6_results.json"),JSON.stringify(report,null,2)+"\n");
function diff(a){
  const ap=join(resultDir,"syntax",a.id+"-expected.txt"),bp=join(resultDir,"syntax",a.id+"-actual.txt");
  writeFileSync(ap,String(a.expected));writeFileSync(bp,String(a.actual));
  const d=spawnSync("diff",["-u","--label","A expected","--label","B actual",ap,bp],{encoding:"utf8"});
  if(d.status!==1)throw new Error(`DIFF row has no text diff ${a.id}`);
  return d.stdout;
}
const detail=["# Assertion measurements","",`Primary: ${primary.PASS} PASS, ${primary.DIFF} DIFF, ${primary.GAP} GAP, ${primary.assertions}/137 inventoried.`,"",`All matchers: ${all.PASS} PASS, ${all.DIFF} DIFF, ${all.GAP} GAP, ${all.assertions}/239 inventoried.`,"","Each row names the original A line. B is evaluated only from the original input. GAP rows have no candidate output. `6_results.json` retains oracle literals, input expressions, enclosing test bodies, parser contexts, and rustfmt output.","","| A file:line | matcher | bucket | status | byte equality | strip-whitespace equality | reason | parse ERROR/MISSING | rustfmt exit |","|---|---|---|---|---|---|---|---|---|"];
for(const a of rows)detail.push(`| ${a.file}:${a.line} | ${a.matcher} | ${a.primary?"primary":"supplemental"} | ${a.status} | ${a.byteEqual??"n/a"} | ${a.whitespaceEqual??"n/a"} | ${a.reason??"none"} | ${a.syntax?`${a.syntax.ERROR}/${a.syntax.MISSING}`:"n/a"} | ${a.syntax?.rustfmtExit??"n/a"} |`);
for(const a of rows.filter(a=>a.status!=="PASS")){
  detail.push("",`## ${a.file}:${a.line} (${a.status})`,"",`${a.reason}: ${a.detail??"Text differs from A."}`);
  if(a.status==="DIFF")detail.push("","```diff",diff(a).trimEnd(),"```");
}
detail.push("","## All files from multi-file render calls","","These checks also cover files with no A content assertion. Repeated file paths belong to separate test renders.","","| A test file:line | B path | ERROR/MISSING | rustfmt exit |","|---|---|---|---|");
for(const f of fileOutputs)detail.push(`| ${f.referenceFile}:${f.referenceLine} | ${f.path} | ${f.syntax.ERROR}/${f.syntax.MISSING} | ${f.syntax.rustfmtExit} |`);
writeFileSync(join(here,"6_results.md"),detail.join("\n")+"\n");
function lines(path){return readFileSync(path,"utf8").split("\n").length-1;}
function list(dir){const out=[];for(const e of readdirSync(dir,{withFileTypes:true})){const p=join(dir,e.name);if(e.isDirectory())out.push(...list(p));else out.push(p);}return out;}
const refRoot=process.env.ALLOY_RUST_REFERENCE??"/Users/chrishafley/projects/hafley-tsp/packages/rust/src";
const handA=existsSync(refRoot)?list(refRoot).filter(p=>/\.tsx?$/.test(p)&&!p.includes(".test.")):[];
const sourceLinesA=handA.reduce((n,p)=>n+lines(p),0);
const handB=["rust/0_subset.mjs","rust/0_name-policy.ts","rust/1_scope.ts","rust/2_print.tsx","rust/3_SourceFile.tsx","rust/locals.scm"];
const counts=[...handB.map(f=>`| hand Rust policy | ${f} | ${lines(join(here,f))} |`),`| hand twin intent adapters | 2_components.tsx | ${lines(join(here,"2_components.tsx"))} |`,`| generated Rust, 163 components | gen/rust/0_nodes.tsx | ${lines(join(here,"gen/rust/0_nodes.tsx"))} |`,`| copied generator | 0_gen.mjs | ${lines(join(here,"0_gen.mjs"))} |`,`| copied shared printer | core/0_print.tsx | ${lines(join(here,"core/0_print.tsx"))} |`,`| copied fallback policy | core/1_plain.tsx | ${lines(join(here,"core/1_plain.tsx"))} |`,`| A hand implementation (${handA.length} files, excludes tests) | packages/rust/src | ${sourceLinesA} |`,`| A oracle test fixtures (27 files) | fixtures/ | ${list(join(here,"fixtures")).reduce((n,p)=>n+lines(p),0)} |`,`| B twins (27 files) | twins/ | ${list(join(here,"twins")).reduce((n,p)=>n+lines(p),0)} |`];
const table=perFile.map(r=>`| ${r.file} | ${r.primary.assertions} | ${r.primary.PASS} | ${r.primary.DIFF} | ${r.primary.GAP} | ${r.assertions} | ${r.PASS} | ${r.DIFF} | ${r.GAP} | ${r.topGap} |`);
const readme=`# alloy-scm Rust vs hafley-tsp Rust\n\nA: read-only /Users/chrishafley/projects/hafley-tsp/packages/rust, hand-written Alloy Rust on @alloy-js/core 0.23.0-dev.12. B: this isolated lab, generated from hafley_scm's tree-sitter-rust 0.24.2 grammar.json and node-types.json plus hand Rust locals.scm and policy.\n\nRequested denominator: **${primary.PASS} PASS, ${primary.DIFF} DIFF, ${primary.GAP} GAP; 137/137 inventoried**. Including declaration renders and other matchers: **${all.PASS} PASS, ${all.DIFF} DIFF, ${all.GAP} GAP; 239/239 inventoried**. Strip-whitespace comparison (remove every \`\\s\` character from measured strings): **${whitespace.primary.PASS} PASS, ${whitespace.primary.DIFF} DIFF, ${whitespace.primary.GAP} GAP** primary; **${whitespace.all.PASS} PASS, ${whitespace.all.DIFF} DIFF, ${whitespace.all.GAP} GAP** all matchers. Scalar and presence matchers retain their measured status.\n\nBefore/after totals (239 matchers; PASS/DIFF/GAP):\n\n| step | byte PASS/DIFF/GAP | strip-whitespace PASS/DIFF/GAP |\n|---|---|---|\n| Before | 87/30/122 | 101/16/122 |\n| 1. Comparison columns | 87/30/122 | 101/16/122 |\n| 2. Printer choice | 96/21/122 | 108/9/122 |\n| 3. Named Rust props | 97/20/122 | 108/9/122 |\n| 4. Module registry | ${all.PASS}/${all.DIFF}/${all.GAP} | ${whitespace.all.PASS}/${whitespace.all.DIFF}/${whitespace.all.GAP} |\n\nPrimary baseline: **30/5/102** for both comparisons; current primary: **${primary.PASS}/${primary.DIFF}/${primary.GAP}** byte and **${whitespace.primary.PASS}/${whitespace.primary.DIFF}/${whitespace.primary.GAP}** strip-whitespace.\n\nAll 27 A test files have B twins. A has 101 inline snapshots and 36 toBe assertions (137), plus 85 toRenderTo, 9 toBeNull, 6 toContain, and 2 toEqual assertions (102).\n\n## Run\n\n\`pnpm install --ignore-workspace && pnpm test\`\n\nPrerequisites: Node/pnpm, Cargo with the worktree grammar dependencies already fetched, rustfmt, and the existing ryii CLI (tree-sitter query). No boop or hafley-rs Cargo tests run. Generation uses cargo metadata, without building a crate. Tests use the existing dependency installation through an ignored local symlink in this run; a fresh pnpm install creates local dependencies.\n\n\`pnpm test\` regenerates Rust, type-checks the lab, clears stale measurements, runs all twins, verifies the assertion inventory, checks emitted Rust, and regenerates this README and the reports. Exit 0 means the measurement harness completed; PASS/DIFF/GAP and syntax results are the comparison result. It does not require every parity assertion to pass.\n\nTo refresh twins after a deliberate change in A: \`node 3_twins.mjs /path/to/hafley-tsp/packages/rust/src\`. Original source is copied byte-for-byte to fixtures/ and hashed. Expected literals are copied verbatim to 3_assertions.json and runnable twins. Unsupported pipeline twins retain inputs and expected literals in their linked descriptors and fixtures; they explicitly record GAP rather than executing A.\n\n## Reuse and boundaries\n\n0_gen.mjs and core/ were copied from ../the-gang-tries-to-make-alloy-turnkey. The lab-local core/0_print.tsx now diverges to retain separator and terminator literals. The generator resolves policy/subset/locals beside itself, so copying keeps B isolated. All 163 named concrete Rust kinds are selected. The lab-local generator also diverges to lower Rust policy-selected where_clause and mutable_specifier children into named props. The fallback policy remains byte-equal to the source lab. No edits to A or the turnkey lab.\n\n2_components.tsx is a hand-written test adapter from A's high-level declaration props to generated nodes. Attribute strings, raw type/expression children, self parameters, async modifiers, and field/variant trailing commas use caller text where the grammar exposes opaque children. These are counted as hand-written code. Successful cases therefore measure generated nodes plus this adapter and policy. Expected output never supplies candidate input.\n\nThe policy uses per-file declaration scopes and a reactive module registry. CrateDirectory and ModDirectory own child module sets; module-root SourceFiles emit sorted pub mod declarations. Cross-file references register deduplicated, sorted crate-qualified use statements and emit the symbol name at the usage site. The lab has no named-type/function symbol metadata factories, TypeSpec adapter, operation planner, file zones, or endpoint/routing/daemon pipeline. Related tests remain GAPs. No pipeline GAP is counted as a syntax or equality pass. Remaining strip-whitespace DIFFs are where clauses: A emits a trailing predicate comma and B omits it.\n\nThe printer selects complete prop consumption by maximizing comma literals between supplied values and semicolon terminators, then minimizing other literals. The Rust policy accepts the semicolon branch for tuple bodies and the braced branch for field bodies. This retains Rust list separators and tuple-struct terminators. Rust where_clause and mutable_specifier are consumed from named prop queues; visibility and opaque function modifiers remain children. The adapter supplies where clauses without an optional trailing comma.\n\n## Validation\n\nEvery measured emitted string and every file produced by a multi-file render call has a tree-sitter-rust query for both ERROR and MISSING plus \`rustfmt --check --edition 2021 --config skip_children=true\`. skip_children prevents rustfmt from resolving modules into unrelated files. Default rustfmt indentation is preserved. Fragments are embedded in syntax contexts: reference types in a type alias, let bindings in a function, serde fragments as an attribute, documentation attached to an item, and name-policy outputs in an appropriate declaration. Each exact context is recorded. Scalar booleans/null/presence checks have no Rust text to parse. The original A expected text receives the same checks. No Rust compilation or type-check claim is made.\n\nB: **${syntaxTotals.parseClean}/${syntaxTotals.outputs} contexts have 0 ERROR/MISSING**, ${syntaxTotals.parseFailed} fail parsing; **${syntaxTotals.rustfmtClean}/${syntaxTotals.outputs} pass rustfmt check**, ${syntaxTotals.rustfmtFailed} fail. Multi-file renders additionally produced ${fileSyntaxTotals.outputs} files, including unasserted outputs: ${fileSyntaxTotals.parseClean}/${fileSyntaxTotals.outputs} have zero ERROR/MISSING and ${fileSyntaxTotals.rustfmtClean}/${fileSyntaxTotals.outputs} pass rustfmt check. A formatting and invalid raw-identifier fragment results remain available beside B results. Byte equality and syntax validity are recorded separately.\n\nTools used: ${execFileSync('rustfmt',['--version'],{encoding:'utf8'}).trim()}; ${execFileSync('ryii',['--version'],{encoding:'utf8'}).trim()}.\n\n## Per A test file\n\nPrimary columns use the requested 137 denominator; all columns include supplemental matchers. Top GAP reason is computed across all matchers.\n\n| A test file | primary assertions | PASS | DIFF | GAP | all assertions | PASS | DIFF | GAP | top GAP reason |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|---|\n${table.join('\n')}\n| TOTAL | ${primary.assertions} | ${primary.PASS} | ${primary.DIFF} | ${primary.GAP} | ${all.assertions} | ${all.PASS} | ${all.DIFF} | ${all.GAP} | |\n\n## Line counts\n\nA implementation lines cover its full emitter, adapters, components, symbols, and scopes. B generated nodes cover grammar syntax; B's absent pipelines are listed above.\n\n| category | path | lines |\n|---|---|---:|\n${counts.join('\n')}\n\n[Per-assertion outcomes and exact diffs](6_results.md), [machine-readable outcomes and syntax diagnostics](6_results.json), [immutable assertion inventory](3_assertions.json).\n`;
writeFileSync(join(here,"README.md"),readme);
console.log(JSON.stringify({primary,all,whitespace,syntax:syntaxTotals,fileSyntax:fileSyntaxTotals}));
