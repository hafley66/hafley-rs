import { describe, expect, it } from "vitest";
import { execSync } from "node:child_process";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { emitCrate } from "./03_emit-crate.js";
import type { TypeDef } from "./00_types.js";

const OUTPUT_DIR = join(import.meta.dirname, "../../test-output-emitter");

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

const USER_MODEL: TypeDef = {
  kind: "model",
  name: "User",
  properties: [
    { name: "id", type: { kind: "scalar", name: "int64" } },
    { name: "name", type: { kind: "scalar", name: "string" } },
    { name: "email", type: { kind: "scalar", name: "string" } },
    { name: "age", type: { kind: "scalar", name: "int32" }, optional: true },
  ],
};

const POST_MODEL: TypeDef = {
  kind: "model",
  name: "BlogPost",
  properties: [
    { name: "id", type: { kind: "scalar", name: "int64" } },
    { name: "title", type: { kind: "scalar", name: "string" } },
    { name: "body", type: { kind: "scalar", name: "string" } },
    { name: "author_id", type: { kind: "scalar", name: "int64" } },
    { name: "tags", type: { kind: "array", element: { kind: "scalar", name: "string" } } },
    { name: "created_at", type: { kind: "scalar", name: "utcDateTime" } },
    { name: "metadata", type: { kind: "map", key: { kind: "scalar", name: "string" }, value: { kind: "scalar", name: "string" } } },
  ],
};

const STATUS_ENUM: TypeDef = {
  kind: "enum",
  name: "PostStatus",
  members: [
    { name: "Draft" },
    { name: "Published" },
    { name: "Archived" },
  ],
};

describe("emitter", () => {
  it("emits a single model to correct Rust", () => {
    const res = emitCrate([USER_MODEL]);
    const file = findFile(res, "models/user.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use serde::Deserialize;
      use serde::Serialize;

      #[derive(Debug, Clone, Serialize, Deserialize)]
      pub struct User {
        pub id: i64,
        pub name: String,
        pub email: String,
        pub age: Option<i32>,
      }"
    `);
  });

  it("emits an enum to correct Rust", () => {
    const res = emitCrate([STATUS_ENUM]);
    const file = findFile(res, "models/post_status.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use serde::Deserialize;
      use serde::Serialize;

      #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
      pub enum PostStatus {
        Draft,
        Published,
        Archived,
      }"
    `);
  });

  it("emits multiple types with correct mod.rs and lib.rs", () => {
    const res = emitCrate([USER_MODEL, POST_MODEL, STATUS_ENUM]);

    const modRs = findFile(res, "models/mod.rs");
    expect(modRs.contents.trim()).toMatchInlineSnapshot(`
      "pub mod blog_post;
      pub mod post_status;
      pub mod user;"
    `);

    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`"pub mod models;"`);
  });

  it("emits complex types (array, map, datetime, optional)", () => {
    const res = emitCrate([POST_MODEL]);
    const file = findFile(res, "models/blog_post.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use chrono::DateTime;
      use chrono::Utc;
      use serde::Deserialize;
      use serde::Serialize;
      use std::collections::HashMap;

      #[derive(Debug, Clone, Serialize, Deserialize)]
      pub struct BlogPost {
        pub id: i64,
        pub title: String,
        pub body: String,
        pub author_id: i64,
        pub tags: Vec<String>,
        pub created_at: DateTime<Utc>,
        pub metadata: HashMap<String, String>,
      }"
    `);
  });

  it("emits uuid, decimal, offsetDateTime, safeint scalars", () => {
    const ENTITY: TypeDef = {
      kind: "model",
      name: "Entity",
      properties: [
        { name: "id", type: { kind: "scalar", name: "uuid" } },
        { name: "price", type: { kind: "scalar", name: "decimal" } },
        { name: "big_price", type: { kind: "scalar", name: "decimal128" } },
        { name: "count", type: { kind: "scalar", name: "integer" } },
        { name: "rating", type: { kind: "scalar", name: "float" } },
        { name: "score", type: { kind: "scalar", name: "numeric" } },
        { name: "js_id", type: { kind: "scalar", name: "safeint" } },
        { name: "modified_at", type: { kind: "scalar", name: "offsetDateTime" } },
      ],
    };
    const res = emitCrate([ENTITY]);
    const file = findFile(res, "models/entity.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use chrono::DateTime;
      use chrono::FixedOffset;
      use rust_decimal::Decimal;
      use serde::Deserialize;
      use serde::Serialize;
      use uuid::Uuid;

      #[derive(Debug, Clone, Serialize, Deserialize)]
      pub struct Entity {
        pub id: Uuid,
        pub price: Decimal,
        pub big_price: Decimal,
        pub count: i64,
        pub rating: f64,
        pub score: f64,
        pub js_id: i64,
        pub modified_at: DateTime<FixedOffset>,
      }"
    `);
  });

  it("cross-model reference generates use crate::", () => {
    const POST_WITH_AUTHOR: TypeDef = {
      kind: "model",
      name: "BlogPost",
      properties: [
        { name: "id", type: { kind: "scalar", name: "int64" } },
        { name: "title", type: { kind: "scalar", name: "string" } },
        { name: "author", type: { kind: "model", name: "User" } },
        { name: "status", type: { kind: "enum", name: "PostStatus" } },
        { name: "reviewers", type: { kind: "array", element: { kind: "model", name: "User" } } },
        { name: "co_author", type: { kind: "model", name: "User" }, optional: true },
      ],
    };

    const res = emitCrate([USER_MODEL, POST_WITH_AUTHOR, STATUS_ENUM]);

    const postFile = findFile(res, "models/blog_post.rs");
    expect(postFile.contents.trim()).toMatchInlineSnapshot(`
      "use serde::Deserialize;
      use serde::Serialize;

      use crate::models::post_status::PostStatus;
      use crate::models::user::User;

      #[derive(Debug, Clone, Serialize, Deserialize)]
      pub struct BlogPost {
        pub id: i64,
        pub title: String,
        pub author: User,
        pub status: PostStatus,
        pub reviewers: Vec<User>,
        pub co_author: Option<User>,
      }"
    `);
  });

  it.skipIf(!process.env.CARGO_CHECK)("cross-refs compile with cargo check", () => {
    const POST_WITH_AUTHOR: TypeDef = {
      kind: "model",
      name: "BlogPost",
      properties: [
        { name: "id", type: { kind: "scalar", name: "int64" } },
        { name: "title", type: { kind: "scalar", name: "string" } },
        { name: "author", type: { kind: "model", name: "User" } },
        { name: "status", type: { kind: "enum", name: "PostStatus" } },
        { name: "reviewers", type: { kind: "array", element: { kind: "model", name: "User" } } },
        { name: "co_author", type: { kind: "model", name: "User" }, optional: true },
      ],
    };

    const res = emitCrate([USER_MODEL, POST_WITH_AUTHOR, STATUS_ENUM]);

    rmSync(OUTPUT_DIR, { recursive: true, force: true });
    mkdirSync(join(OUTPUT_DIR, "src"), { recursive: true });

    writeFileSync(join(OUTPUT_DIR, "Cargo.toml"), `[package]
name = "emitter-test"
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
