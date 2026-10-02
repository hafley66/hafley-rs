import {
  List,
  Output,
  refkey,
  render,
} from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { execSync } from "node:child_process";
import { mkdirSync, writeFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { VisibilityContext } from "../../scopes/06_contexts.js";
import { StructDeclaration, StructField } from "../1_declarations/0_StructDeclaration.js";
import { SourceFile } from "../3_files/0_SourceFile.js";
import { Reference } from "../2_references/0_Reference.js";
import { CrateDirectory } from "../3_files/1_CrateDirectory.js";
import { ModDirectory } from "../3_files/2_ModDirectory.js";
import { ZoneProvider, Zone } from "../4_codegen/0_AppendZone.js";
import { Endpoint } from "../4_codegen/2_Endpoint.js";
import { FunctionDeclaration } from "../1_declarations/4_FunctionDeclaration.js";

const OUTPUT_DIR = join(import.meta.dirname, "../../test-output-endpoint");

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

describe("Endpoint component", () => {
  const userKey = refkey();
  const postKey = refkey();

  // Same layout as before, but using <Endpoint /> instead of manual wiring:
  //   src/
  //     lib.rs
  //     user.rs
  //     list_users_auto.rs   ← from Endpoint
  //     list_users.rs        ← from Endpoint
  //     orgs/
  //       post.rs
  //       list_org_posts_auto.rs  ← from Endpoint
  //       list_org_posts.rs       ← from Endpoint

  function buildTree() {
    return render(
      <Output>
        <VisibilityContext.Provider value="pub">
          <ZoneProvider>
            <CrateDirectory>
              {/* Models */}
              <SourceFile path="user.rs" externalUses={["serde::Deserialize", "serde::Serialize"]}>
                <StructDeclaration name="User" refkey={userKey} derive={["Debug", "Clone", "Serialize", "Deserialize"]}>
                  <List hardline>
                    <StructField name="id" type="i64" />
                    <StructField name="name" type="String" />
                  </List>
                </StructDeclaration>
              </SourceFile>

              {/* Endpoint 1: GET /users (depth 1) */}
              <Endpoint
                path="/users"
                method="get"
                name="list_users"
                routePath="list_users_auto"
                responseModel={userKey}
                responseList
              />

              {/* Orgs domain */}
              <ModDirectory name="orgs">
                <SourceFile path="post.rs" externalUses={["serde::Deserialize", "serde::Serialize"]}>
                  <StructDeclaration name="Post" refkey={postKey} derive={["Debug", "Clone", "Serialize", "Deserialize"]}>
                    <List hardline>
                      <StructField name="id" type="i64" />
                      <StructField name="title" type="String" />
                      <StructField name="author" type={<Reference refkey={userKey} />} />
                    </List>
                  </StructDeclaration>
                </SourceFile>

                {/* Endpoint 2: GET /orgs/:id/posts (depth 2) */}
                <Endpoint
                  path="/orgs/:id/posts"
                  method="get"
                  name="list_org_posts"
                  routePath="orgs::list_org_posts_auto"
                  responseModel={postKey}
                  responseList
                  params={[{
                    extractor: "Path(org_id)",
                    extractorType: "Path<i64>",
                    name: "org_id",
                    type: "i64",
                  }]}
                />
              </ModDirectory>

              {/* Router in lib.rs */}
              <SourceFile path="lib.rs" externalUses={["axum::Router", "axum::routing::get"]}>
                <FunctionDeclaration name="router" returns="Router">
                  Router::new()<Zone name="router" />
                </FunctionDeclaration>
              </SourceFile>
            </CrateDirectory>
          </ZoneProvider>
        </VisibilityContext.Provider>
      </Output>
    );
  }

  it("auto file: axum wrapper with delegation", () => {
    const res = buildTree();
    const file = findFile(res, "list_users_auto.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;

      use crate::user::User;

      pub async fn list_users() -> Json<Vec<User>> {
        Json(crate::list_users::list_users_impl().await)
      }"
    `);
  });

  it("manual file: pure domain stub", () => {
    const res = buildTree();
    const file = findFile(res, "list_users.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use crate::user::User;

      pub async fn list_users_impl() -> Vec<User> {
        todo!()
      }"
    `);
  });

  it("deeper auto file: extractor + delegation with args", () => {
    const res = buildTree();
    const file = findFile(res, "orgs/list_org_posts_auto.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;
      use axum::extract::Path;

      use crate::orgs::post::Post;

      pub async fn list_org_posts(Path(org_id): Path<i64>) -> Json<Vec<Post>> {
        Json(crate::orgs::list_org_posts::list_org_posts_impl(org_id).await)
      }"
    `);
  });

  it("deeper manual file: plain params, no framework", () => {
    const res = buildTree();
    const file = findFile(res, "orgs/list_org_posts.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use crate::orgs::post::Post;

      pub async fn list_org_posts_impl(org_id: i64) -> Vec<Post> {
        let _ = org_id;
        todo!()
      }"
    `);
  });

  it("router collects from both endpoints", () => {
    const res = buildTree();
    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod list_users;
      pub mod list_users_auto;
      pub mod orgs;
      pub mod user;

      use axum::Router;
      use axum::routing::get;

      pub fn router() -> Router {
        Router::new()
        .route("/users", get(list_users_auto::list_users))
        .route("/orgs/:id/posts", get(orgs::list_org_posts_auto::list_org_posts))
      }"
    `);
  });

  it("compiles with cargo check, no unused vars", () => {
    const res = buildTree();

    rmSync(OUTPUT_DIR, { recursive: true, force: true });
    mkdirSync(join(OUTPUT_DIR, "src"), { recursive: true });

    writeFileSync(join(OUTPUT_DIR, "Cargo.toml"), `[package]
name = "endpoint-test"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
axum = "0.7"
tokio = { version = "1", features = ["full"] }
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
