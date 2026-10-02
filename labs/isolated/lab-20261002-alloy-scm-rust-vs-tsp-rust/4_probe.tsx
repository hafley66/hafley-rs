import { printTree, renderTree, render as coreRender } from "@alloy-js/core";
import { expect } from "vitest";
import { mkdirSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { Gap } from "./1_gap.js";
import assertions from "./3_assertions.json" with { type: "json" };
const here = dirname(new URL(import.meta.url).pathname);
const byId = new Map(assertions.map(a => [a.id, a]));
const completed = new Set<string>();
let activeTest="";
let renderOrdinal=0;
function save(id: string, data: Record<string, unknown>) {
  if(completed.has(id)) throw new Error(`Duplicate result ${id}`);
  completed.add(id); mkdirSync(join(here,".results"),{recursive:true});
  writeFileSync(join(here,".results",id+".json"),JSON.stringify({id,...data},null,2)+"\n");
}
export function gap(id: string, reason: string, detail: string) { save(id, {status:"GAP",reason,detail}); }
export function probeTest(ids: string[], run: () => unknown) {
  activeTest=ids[0]; renderOrdinal=0;
  try { run(); } catch(e) {
    if(!(e instanceof Gap)) throw e;
    for(const id of ids) if(!completed.has(id)) gap(id,e.reason,e.message);
  }
  for(const id of ids) if(!completed.has(id)) throw new Error(`Assertion was not evaluated: ${id}`);
}
export function probe(id: string, input: () => any, expected: any) {
  const a = byId.get(id)!;
  let actual: any;
  try { actual=input(); } catch(e) { if(e instanceof Gap) { gap(id,e.reason,e.message);return; } throw e; }
  try {
    if(a.matcher === "toRenderTo") actual=printTree(renderTree(actual));
    let assertion:any=expect(actual);
    if(a.negated) assertion=assertion.not;
    if(a.matcher === "toRenderTo") assertion.toBe(dedent(expected));
    else if(a.matcher === "toMatchInlineSnapshot") assertion.toBe(snapshotString(expected));
    else assertion[a.matcher](expected);
    save(id,{status:"PASS",expected:a.matcher==="toRenderTo"?dedent(expected):a.matcher==="toMatchInlineSnapshot"?snapshotString(expected):expected,actual:a.matcher==="toBeNull"?Boolean(actual):actual});
  } catch(e:any) {
    if(e instanceof Gap) {gap(id,e.reason,e.message);return;}
    // Alloy propagates a component's Gap through reactive error causes.
    if(e.cause instanceof Gap) {gap(id,e.cause.reason,e.cause.message);return;}
    if(!("actual" in e) || !("expected" in e)) throw e;
    save(id,{status:"DIFF",reason:a.file.includes("2_references")||a.file.includes("3_files")?"refkey/import resolution":"whitespace/blank-line policy",expected:e.expected,actual:e.actual,detail:e.message});
  }
}
function dedent(s:string):string {
  s=s.replace(/^\n|\n[ ]*$/g,""); const indent=s.match(/^[ \t]+/)?.[0]??"";
  return s.split("\n").map(l=>l.startsWith(indent)?l.slice(indent.length):l).join("\n");
}

function snapshotString(s:string):string {
  const text=dedent(s).trim();
  if(!text.startsWith('"')||!text.endsWith('"')) throw new Error("Only string snapshots are evaluated by the candidate component twins");
  return text.slice(1,-1);
}

// Capture every generated file, including outputs that A does not assert.
export function render(...args: Parameters<typeof coreRender>) {
  const result=coreRender(...args);
  const ordinal=renderOrdinal++;
  let fileOrdinal=0;
  function visit(node:any) {
    if(node.kind === "file") {
      const output={testId:activeTest,path:node.path,text:node.contents};
      const dir=join(here,".results","files");mkdirSync(dir,{recursive:true});
      writeFileSync(join(dir,`${activeTest}-${ordinal}-${fileOrdinal++}.json`),JSON.stringify(output,null,2)+"\n");
    } else if(Array.isArray(node.contents)) for(const child of node.contents) visit(child);
  }
  visit(result);return result;
}
