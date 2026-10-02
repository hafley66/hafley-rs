import { describe, expect, it } from "vitest";
import { execSync } from "node:child_process";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { programToTypeDefs } from "./00_typespec-to-neutral.js";
import { emitCrate } from "../emitter/03_emit-crate.js";

// Mock builders
function mockScalar(name: string, baseScalar?: any): any {
  return { kind: "Scalar", name, baseScalar };
}
function mockModelProperty(name: string, type: any, optional = false): any {
  return { kind: "ModelProperty", name, type, optional };
}
function mockModel(name: string, properties: any[], indexer?: any): any {
  return { kind: "Model", name, properties: new Map(properties.map((p: any) => [p.name, p])), indexer };
}
function mockEnum(name: string, members: { name: string; value?: string | number }[]): any {
  return { kind: "Enum", name, members: new Map(members.map(m => [m.name, { kind: "EnumMember", name: m.name, value: m.value }])) };
}
function mockNamespace(name: string, models: any[] = [], enums: any[] = [], namespaces: any[] = []): any {
  return {
    kind: "Namespace", name,
    models: new Map(models.map((m: any) => [m.name, m])),
    enums: new Map(enums.map((e: any) => [e.name, e])),
    namespaces: new Map(namespaces.map((ns: any) => [ns.name, ns])),
  };
}

const OUTPUT_DIR = join(import.meta.dirname, "../../test-output-typespec-adapter");

function findFile(node: any, path: string): any {
  for (const item of node.contents) {
    if (item.kind === "file" && item.path === path) return item;
    if (item.kind === "directory") {
      const found = findFile(item, path);
      if (found) return found;
    }
  }
  return null;
}

function writeTree(node: any, dir: string) {
  for (const item of node.contents) {
    if (item.kind === "directory") {
      const subdir = join(dir, item.path);
      mkdirSync(subdir, { recursive: true });
      writeTree(item, dir);
    } else if (item.kind === "file") {
      const filePath = join(dir, item.path);
      mkdirSync(join(filePath, ".."), { recursive: true });
      writeFileSync(filePath, item.contents);
    }
  }
}

describe("typespec adapter -> emitCrate integration", () => {
  // Simulate a TypeSpec program with:
  // - User model with scalar fields + optional
  // - Address model referenced by User
  // - Role enum referenced by User
  // - Team model with array of User
  // - Metadata map on Team
  const stringScalar = mockScalar("string");
  const uuidScalar = mockScalar("uuid", stringScalar);

  const address = mockModel("Address", [
    mockModelProperty("street", stringScalar),
    mockModelProperty("city", stringScalar),
    mockModelProperty("zip", stringScalar),
  ]);

  const roleEnum: any = { kind: "Enum", name: "Role", members: new Map([
    ["Admin", { kind: "EnumMember", name: "Admin" }],
    ["Member", { kind: "EnumMember", name: "Member" }],
    ["Guest", { kind: "EnumMember", name: "Guest" }],
  ]) };

  const user = mockModel("User", [
    mockModelProperty("id", uuidScalar),
    mockModelProperty("name", stringScalar),
    mockModelProperty("email", stringScalar, true),
    mockModelProperty("address", address),
    mockModelProperty("role", roleEnum),
  ]);

  const usersArray = mockModel("Array", [], { key: mockScalar("integer"), value: user });
  const metadataRecord = mockModel("Record", [], { key: stringScalar, value: stringScalar });

  const team = mockModel("Team", [
    mockModelProperty("name", stringScalar),
    mockModelProperty("members", usersArray),
    mockModelProperty("metadata", metadataRecord),
    mockModelProperty("created_at", mockScalar("utcDateTime")),
  ]);

  const serviceNs = mockNamespace("MyService", [address, user, team], [roleEnum]);
  const typescriptNs = mockNamespace("TypeSpec", [mockModel("Array", []), mockModel("Record", [])]);
  const globalNs = mockNamespace("", [], [], [typescriptNs, serviceNs]);

  const program = { getGlobalNamespaceType: () => globalNs };

  it("full pipeline: TypeSpec mock -> neutral types -> Rust crate", () => {
    const defs = programToTypeDefs(program);
    expect(defs.map(d => d.name).sort()).toMatchInlineSnapshot(`
      [
        "Address",
        "Role",
        "Team",
        "User",
      ]
    `);

    const res = emitCrate(defs);

    const userFile = findFile(res, "models/user.rs");
    expect(userFile.contents.trim()).toMatchInlineSnapshot(`
      "use serde::Deserialize;
      use serde::Serialize;

      use crate::models::address::Address;
      use crate::models::role::Role;

      #[derive(Debug, Clone, Serialize, Deserialize)]
      pub struct User {
        pub id: String,
        pub name: String,
        pub email: Option<String>,
        pub address: Address,
        pub role: Role,
      }"
    `);

    const teamFile = findFile(res, "models/team.rs");
    expect(teamFile.contents.trim()).toMatchInlineSnapshot(`
      "use chrono::DateTime;
      use chrono::Utc;
      use serde::Deserialize;
      use serde::Serialize;
      use std::collections::HashMap;

      use crate::models::user::User;

      #[derive(Debug, Clone, Serialize, Deserialize)]
      pub struct Team {
        pub name: String,
        pub members: Vec<User>,
        pub metadata: HashMap<String, String>,
        pub created_at: DateTime<Utc>,
      }"
    `);

    const roleFile = findFile(res, "models/role.rs");
    expect(roleFile.contents.trim()).toMatchInlineSnapshot(`
      "use serde::Deserialize;
      use serde::Serialize;

      #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
      pub enum Role {
        Admin,
        Member,
        Guest,
      }"
    `);

    const modRs = findFile(res, "models/mod.rs");
    expect(modRs.contents.trim()).toMatchInlineSnapshot(`
      "pub mod address;
      pub mod role;
      pub mod team;
      pub mod user;"
    `);
  });

  it.skipIf(!process.env.CARGO_CHECK)("compiles with cargo check", () => {
    const defs = programToTypeDefs(program);
    const res = emitCrate(defs);

    rmSync(OUTPUT_DIR, { recursive: true, force: true });
    mkdirSync(join(OUTPUT_DIR, "src"), { recursive: true });

    writeFileSync(join(OUTPUT_DIR, "Cargo.toml"), `[package]
name = "typespec-adapter-test"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
chrono = { version = "0.4", features = ["serde"] }
`);

    writeTree(res, join(OUTPUT_DIR, "src"));

    try {
      execSync("cargo check 2>&1", {
        cwd: OUTPUT_DIR,
        timeout: 120000,
        encoding: "utf-8",
      });
      expect(true).toBe(true);
    } catch (e: any) {
      console.error("cargo check failed:\n", e.stdout ?? e.stderr ?? e.message);
      expect.fail("cargo check failed -- see output above");
    }
  }, 120000);
});
