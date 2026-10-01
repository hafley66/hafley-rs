import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import { emitTsp } from "./1_demo_typespec.js";

const here = dirname(new URL(import.meta.url).pathname);
const worktree = resolve(here, "../../..");
const out = join(here, "out");
const file = join(out, "api.tsp");
const RYII = join(worktree, "crates/sprefa-extract/target/release/ryii");
const TSP = "/Users/chrishafley/projects/boop2/node_modules/.bin/tsp";

const run = (cmd: string, args: string[], cwd = here) => {
  try {
    return { status: 0, stdout: execFileSync(cmd, args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }) };
  } catch (e: any) { return { status: e.status, stdout: String(e.stdout) + String(e.stderr) }; }
};
const rows = (query: string) => {
  const r = run(RYII, ["query", "--query", query, file], worktree);
  return { status: r.status, rows: r.stdout.split("\n").filter(Boolean) };
};

describe("turnkey alloy: TypeSpec (grammar from crates/tree-sitter-typespec)", () => {
  let tsp: string;
  beforeAll(() => {
    // ryii is built from crates/sprefa-extract: cargo build --release --features cli,read --bin ryii
    if (!existsSync(RYII)) throw new Error(`missing ${RYII}`);
    tsp = emitTsp();
    mkdirSync(out, { recursive: true });
    writeFileSync(file, tsp);
  });

  it("emits the spec", () => {
    expect(tsp).toMatchInlineSnapshot(`
      "scalar UnixMs extends int64;
      enum Status {
        Active,
        Disabled,
      }
      model User {
        id: int64;
        createdAt: UnixMs;
        status: Status;
      }
      interface UserStore {
        op get(id: int64): User;
      }
      "
    `);
  });

  it("ryii (tree-sitter-typespec) finds 0 ERROR and 0 MISSING nodes", () => {
    expect({ error: rows("(ERROR) @e"), missing: rows("(MISSING) @m") }).toMatchInlineSnapshot(`
      {
        "error": {
          "rows": [],
          "status": 0,
        },
        "missing": {
          "rows": [],
          "status": 0,
        },
      }
    `);
  });

  it("tsp compile (@typespec/compiler 1.10.0) succeeds", () => {
    expect(run(TSP, ["compile", file, "--no-emit", "--warn-as-error", "--pretty=false"])).toMatchInlineSnapshot(`
      {
        "status": 0,
        "stdout": "TypeSpec compiler v1.10.0


      Compilation completed successfully.

      ",
      }
    `);
  }, 120_000);
});
