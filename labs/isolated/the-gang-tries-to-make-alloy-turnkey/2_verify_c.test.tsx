import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { Children, Output, render } from "@alloy-js/core";
import { beforeAll, describe, expect, it } from "vitest";
import { Language, Node as TsNode, Parser } from "web-tree-sitter";
import { emitHeader } from "./1_demo_c.js";
import { SourceFile } from "./c/3_SourceFile.js";
import * as N from "./gen/c/0_nodes.js";

const here = dirname(new URL(import.meta.url).pathname);
const out = join(here, "out");
let parser: Parser;
const parse = (src: string) => parser.parse(src)!.rootNode;
const walk = (n: TsNode): TsNode[] => [n, ...n.children.flatMap((c) => (c ? walk(c) : []))];
const kinds = (src: string) => walk(parse(src)).filter((n) => n.isNamed).map((n) => n.type);
const pascal = (k: string) => k.split("_").map((s) => s[0].toUpperCase() + s.slice(1)).join("");

// CST -> generated components: fields -> props, unfielded named -> children, keywords -> keywords
function toJsx(n: TsNode): Children {
  const C = (N as Record<string, any>)[pascal(n.type)];
  if (!C) throw new Error(`no component for ${n.type}`);
  if (n.namedChildCount === 0 && !n.type.endsWith("_list") && n.type !== "sized_type_specifier") return <C>{n.text}</C>;
  const props: Record<string, Children[]> = {};
  n.children.forEach((c, i) => {
    if (!c) return;
    const f = n.fieldNameForChild(i);
    if (!c.isNamed) {
      if (["signed", "unsigned", "long", "short"].includes(c.type)) (props.keywords ??= []).push(c.type);
      return;
    }
    const leafText = c.namedChildCount === 0 && f && /identifier$/.test(c.type);
    (props[f ?? "children"] ??= []).push(leafText ? c.text : toJsx(c));
  });
  return <C {...props} />;
}
const renderC = (tree: Children) => {
  const file = render(<Output><SourceFile path="x.h">{tree}</SourceFile></Output>).contents[0] as { contents: string };
  return file.contents;
};

const run = (cmd: string, args: string[]) => {
  try {
    const stderr = execFileSync(cmd, args, {
      encoding: "utf8", stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env, ZIG_GLOBAL_CACHE_DIR: join(here, ".zig-cache"), ZIG_LOCAL_CACHE_DIR: join(here, ".zig-cache") },
    });
    return { status: 0, stderr };
  } catch (e: any) { return { status: e.status, stderr: String(e.stderr) }; }
};

describe("turnkey alloy: C", () => {
  let header: string;
  beforeAll(async () => {
    await Parser.init();
    const pkg = dirname(createRequire(import.meta.url).resolve("tree-sitter-c/package.json"));
    parser = new Parser();
    parser.setLanguage(await Language.load(join(pkg, "tree-sitter-c.wasm")));
    header = emitHeader();
    mkdirSync(out, { recursive: true });
    writeFileSync(join(out, "api.h"), header);
  });

  it("emits the header", () => {
    expect(header).toMatchInlineSnapshot(`
      "#include <stdint.h>

      typedef int64_t UnixMs;
      typedef enum Status {
        Status_Active,
        Status_Disabled
      } Status;
      typedef struct User {
        int64_t id;
        UnixMs created_at;
        Status status;
        char *display_name;
      } User;
      typedef struct UserStore {
        int32_t (*get)(void *self, int64_t id, User *out);
        int32_t (*list)(void *self, User *out, unsigned long cap);
      } UserStore;
      "
    `);
  });

  it("tree-sitter-c parses it with 0 ERROR/MISSING nodes", () => {
    const bad = walk(parse(header)).filter((n) => n.isError || n.isMissing).map((n) => n.toString());
    expect({ bad, hasError: parse(header).hasError }).toMatchInlineSnapshot(`
      {
        "bad": [],
        "hasError": false,
      }
    `);
  });

  it("compiles with cc and zig cc", () => {
    const base = ["-std=c11", "-Wall", "-Wextra", "-x", "c"];
    // zig 0.16.0 cc + -fsyntax-only fails with "FileNotFound"; zig compiles to out/api.o instead
    expect({
      cc: run("cc", [...base, "-fsyntax-only", join(out, "api.h")]),
      zig: run("zig", ["cc", ...base, "-c", join(out, "api.h"), "-o", join(out, "api.o")]),
    }).toMatchInlineSnapshot(`
      {
        "cc": {
          "status": 0,
          "stderr": "",
        },
        "zig": {
          "status": 0,
          "stderr": "",
        },
      }
    `);
  }, 120_000);

  it("emitted CST kind sequence equals the hand-written sample's", () => {
    const sample = readFileSync(join(here, "fixtures/sample.h"), "utf8");
    expect(kinds(header)).toEqual(kinds(sample));
    expect(kinds(header)).toMatchInlineSnapshot(`
      [
        "translation_unit",
        "preproc_include",
        "system_lib_string",
        "type_definition",
        "primitive_type",
        "type_identifier",
        "type_definition",
        "enum_specifier",
        "type_identifier",
        "enumerator_list",
        "enumerator",
        "identifier",
        "enumerator",
        "identifier",
        "type_identifier",
        "type_definition",
        "struct_specifier",
        "type_identifier",
        "field_declaration_list",
        "field_declaration",
        "primitive_type",
        "field_identifier",
        "field_declaration",
        "type_identifier",
        "field_identifier",
        "field_declaration",
        "type_identifier",
        "field_identifier",
        "field_declaration",
        "primitive_type",
        "pointer_declarator",
        "field_identifier",
        "type_identifier",
        "type_definition",
        "struct_specifier",
        "type_identifier",
        "field_declaration_list",
        "field_declaration",
        "primitive_type",
        "function_declarator",
        "parenthesized_declarator",
        "pointer_declarator",
        "field_identifier",
        "parameter_list",
        "parameter_declaration",
        "primitive_type",
        "pointer_declarator",
        "identifier",
        "parameter_declaration",
        "primitive_type",
        "identifier",
        "parameter_declaration",
        "type_identifier",
        "pointer_declarator",
        "identifier",
        "field_declaration",
        "primitive_type",
        "function_declarator",
        "parenthesized_declarator",
        "pointer_declarator",
        "field_identifier",
        "parameter_list",
        "parameter_declaration",
        "primitive_type",
        "pointer_declarator",
        "identifier",
        "parameter_declaration",
        "type_identifier",
        "pointer_declarator",
        "identifier",
        "parameter_declaration",
        "sized_type_specifier",
        "identifier",
        "type_identifier",
      ]
    `);
  });

  it("round-trip: sample CST -> generated components -> print -> same CST kinds", () => {
    const sample = readFileSync(join(here, "fixtures/sample.h"), "utf8");
    const printed = renderC(toJsx(parse(sample)));
    expect(kinds(printed)).toEqual(kinds(sample));
    expect(printed === header.replace(/\n$/, "") || printed === header).toBe(true);
  });
});
