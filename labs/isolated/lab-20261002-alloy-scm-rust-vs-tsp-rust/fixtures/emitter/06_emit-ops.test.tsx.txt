import { compile, NodeHost } from "@typespec/compiler";
import { execSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import { programToOps, type ProgramOps } from "../adapters/02_http-ops.js";
import type { ModelProperty } from "./00_types.js";
import { emitCrate } from "./03_emit-crate.js";
import { writeCrate } from "./05_write-crate.js";

const FIXTURE = join(import.meta.dirname, "../../test/fixtures/ryi_cli");
const PARITY = join(FIXTURE, "parity");

function typeName(t: ModelProperty["type"]): string {
  switch (t.kind) {
    case "scalar": return t.alias ?? t.name;
    case "array": return `${typeName(t.element)}[]`;
    case "map": return `map<${typeName(t.value)}>`;
    default: return t.name;
  }
}

describe("ops -> clap + axum", () => {
  let ops: ProgramOps;
  beforeAll(async () => {
    const program = await compile(NodeHost, join(FIXTURE, "ops.tsp"), { noEmit: true });
    expect(program.diagnostics.map(d => d.message)).toEqual([]);
    ops = programToOps(program);
  });

  it("collects each op once: route, params by source, stream in/out", () => {
    const rows = ops.service.operations.map(op => {
      const params = op.params.map(p =>
        `${p.name}:${p.source}${p.stream ? ":stream" : ""}:${typeName(p.type)}${p.optional ? "?" : ""}${p.default !== undefined ? `=${p.default}` : ""}`,
      );
      const ret = op.returns ? `${op.returnsStream ? "stream " : ""}${typeName(op.returns)}` : "void";
      return `${op.verb} ${op.path} ${op.name}(${params.join(", ")}) -> ${ret}`;
    });
    expect(rows.join("\n")).toMatchInlineSnapshot(`
      "post /fast{?paths,sqlite,lines} fast(paths:query:string[]?, inputs:body:Inputs, sqlite:query:path?, lines:query:boolean?) -> stream TypeEdge
      post /slow{?paths,sqlite,lines,scip_index,no_checker,scip_timeout} slow(paths:query:string[]?, inputs:body:Inputs, sqlite:query:path?, lines:query:boolean?, scip_index:query:path?, no_checker:query:boolean?, scip_timeout:query:uint64?) -> FactSummary
      post /scip{?paths,sqlite,lines,scip_index,scip_cache,scip_timeout,indexer,raw,records,occurrence_text,scip_build} scip(paths:query:string[]?, inputs:body:Inputs, sqlite:query:path?, lines:query:boolean?, scip_index:query:path?, scip_cache:query:path?, scip_timeout:query:uint64?, indexer:query:string?, raw:query:boolean?, records:query:string?, occurrence_text:query:boolean?, scip_build:query:boolean?) -> FactSummary
      post /graph{?paths,callers,uses,from,call_path,type_path,flow_path,sqlite,slow,timeout,at,compare,scip_index,rust_checker,ts_checker,go_checker} graph(paths:query:string[]?, inputs:body:Inputs, callers:query:string?, uses:query:string?, from:query:string?, call_path:query:string?, type_path:query:string?, flow_path:query:string?, sqlite:query:path?, slow:query:boolean?, timeout:query:uint64?=30, at:query:string?, compare:query:string?, scip_index:query:path?, rust_checker:query:boolean?, ts_checker:query:boolean?, go_checker:query:boolean?) -> CallEdge[]
      post /cleave{?target,dest,list,root,state,drag,commit,verify,text_refs,json} cleave(target:query:string?, dest:query:path?, list:query:path?, root:query:path?, state:query:path?, drag:query:boolean?, commit:query:boolean?, verify:query:string?, text_refs:query:boolean?, json:query:boolean?) -> EditPlan
      post /move{?old,new,list,root,verify_cwd,state,commit,shim,relocate_mod,verify,text_refs} move(old:query:path?, new:query:path?, list:query:path?, root:query:path[]?, verify_cwd:query:path?, state:query:path?, commit:query:boolean?, shim:query:boolean?, relocate_mod:query:boolean?, verify:query:string?, text_refs:query:boolean?) -> EditPlan
      post /rename{?target,new,list,root,state,at,commit,text_refs,verify_scip,no_scip_merge,json} rename(target:query:string?, new:query:string?, list:query:path?, root:query:path?, state:query:path?, at:query:uint32?, commit:query:boolean?, text_refs:query:boolean?, verify_scip:query:path?, no_scip_merge:query:boolean?, json:query:boolean?) -> EditPlan
      post /query{?paths,lang,query,digest,sqlite} query(paths:query:string[]?, inputs:body:Inputs, lang:query:string?, query:query:string, digest:query:string?, sqlite:query:path?) -> FactSummary
      post /region/{target}/{id}{?generated,apply,state} region(target:path:path, id:path:string, generated:query:path=-, apply:query:boolean?, state:query:path?) -> EditPlan
      post /watch{?root,patterns,kinds,receipts,once,poll_ms} watch(root:query:path?, patterns:query:string[], kinds:query:string[], receipts:query:path?, once:query:boolean?, poll_ms:query:uint64?=500) -> FactSummary
      post /diff{?root,from,to,patterns,arms,sqlite} diff(root:query:path?, from:query:string, to:query:string, patterns:query:string[], arms:query:string[], sqlite:query:path?) -> FactSummary
      post /ingest{?paths,sqlite} ingest(paths:query:path[]?, input:body:stream:TypeEdge, trace:header:string?, sqlite:query:path?) -> FactSummary
      post /schema schema() -> FactSummary
      post /trail/{runs} trail(runs:path:usize=5) -> FactSummary"
    `);
    expect(ops.types.map(t => t.name).join(" ")).toMatchInlineSnapshot(`"FileArgs Inputs TypeEdge FactSummary CallEdge EditPlan TypeEdgeKind"`);
  });

  it("writes models + ops_auto/cli_auto/http_auto and never rewrites the user-owned ops.rs", () => {
    const written = writeCrate(emitCrate(ops.types, { ops: { service: ops.service, bin: "ryi" } }), join(PARITY, "src"));
    expect(written.join(" ")).toMatchInlineSnapshot(`"models/mod.rs models/file_args.rs models/inputs.rs models/type_edge.rs models/fact_summary.rs models/call_edge.rs models/edit_plan.rs models/type_edge_kind.rs ops_auto.rs cli_auto.rs http_auto.rs lib.rs"`);
    expect(readFileSync(join(PARITY, "src/ops.rs"), "utf8").startsWith("// User-owned")).toBe(true);
    const http = readFileSync(join(PARITY, "src/http_auto.rs"), "utf8");
    expect(http).toContain('headers.get("X-Trace")');
    expect(http).toContain("headers: HeaderMap");
  });

  it("OpenAPI carries the JSONL content type on the stream ops", async () => {
    const out = mkdtempSync(join(tmpdir(), "ryi-openapi-"));
    const program = await compile(NodeHost, join(FIXTURE, "ops.tsp"), {
      emit: ["@typespec/openapi3"],
      outputDir: out,
      options: { "@typespec/openapi3": { "emitter-output-dir": out } },
    });
    const counts = new Map<string, number>();
    for (const d of program.diagnostics) counts.set(`${d.severity} ${d.code}`, (counts.get(`${d.severity} ${d.code}`) ?? 0) + 1);
    const yaml = readFileSync(join(out, readdirSync(out).find(f => f.endsWith(".yaml"))!), "utf8");
    const lines = yaml.split("\n");
    const jsonl = lines.flatMap((line, i) =>
      line.includes("application/jsonl")
        ? [`${lines.slice(0, i).reverse().find(l => /^  \/[^:]*:$/.test(l))?.trim()} ${line.trim()}`]
        : [],
    );
    const diags = [...counts].map(([k, n]) => `${k} x${n}`);
    expect([...diags, ...jsonl].join("\n")).toMatchInlineSnapshot(`
      "/fast: application/jsonl:
      /ingest: application/jsonl:"
    `);
  });

  it.skipIf(!process.env.CARGO_CHECK)("generated crate: clap parity with ryi's 0_cli.rs, CLI + axum streams", () => {
    const out = execSync("cargo test 2>&1", { cwd: PARITY, encoding: "utf8", env: { ...process.env, CARGO_BUILD_JOBS: "4", RUST_TEST_THREADS: "4", RYI_MAX_MEM_MB: "2048", SPREFA_BUILD_GIT_HASH: "test-hash", SPREFA_BUILD_DATETIME: "test-datetime" } });
    expect(out.split("\n").filter(l => l.startsWith("test ") && !l.includes("result")).sort().join("\n")).toMatchInlineSnapshot(`
      "test cli_and_axum_share_ops_streams_and_errors ... ok
      test generated_cli_acceptance_matches_handwritten ... ok
      test generated_cli_matches_handwritten ... ok"
    `);
  }, 60_000);
});
