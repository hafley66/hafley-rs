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
import { FunctionDeclaration } from "../1_declarations/4_FunctionDeclaration.js";
import { SourceFile } from "../3_files/0_SourceFile.js";
import { Reference } from "../2_references/0_Reference.js";
import { CrateDirectory } from "../3_files/1_CrateDirectory.js";
import { ModDirectory } from "../3_files/2_ModDirectory.js";
import { ZoneProvider, Zone, AppendTo } from "../4_codegen/0_AppendZone.js";

const OUTPUT_DIR = join(import.meta.dirname, "../../test-output-axum");

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

describe("axum routing with append zones", () => {
  const userKey = refkey();
  const postKey = refkey();

  // Endpoint 1: GET /users       -> routes/users.rs         (depth 1)
  // Endpoint 2: GET /orgs/:id/posts -> routes/orgs/posts.rs (depth 2)
  // Post model references User for cross-model ref
  function buildTree() {
    return render(
      <Output>
        <VisibilityContext.Provider value="pub">
          <ZoneProvider>
            <CrateDirectory>
              {/* --- Models --- */}
              <ModDirectory name="models">
                <SourceFile path="user.rs" externalUses={["serde::Deserialize", "serde::Serialize"]}>
                  <StructDeclaration name="User" refkey={userKey} derive={["Debug", "Clone", "Serialize", "Deserialize"]}>
                    <List hardline>
                      <StructField name="id" type="i64" />
                      <StructField name="name" type="String" />
                    </List>
                  </StructDeclaration>
                </SourceFile>
                <SourceFile path="post.rs" externalUses={["serde::Deserialize", "serde::Serialize"]}>
                  <StructDeclaration name="Post" refkey={postKey} derive={["Debug", "Clone", "Serialize", "Deserialize"]}>
                    <List hardline>
                      <StructField name="id" type="i64" />
                      <StructField name="title" type="String" />
                      <StructField name="author" type={<Reference refkey={userKey} />} />
                    </List>
                  </StructDeclaration>
                </SourceFile>
              </ModDirectory>

              {/* --- Route depth 1: routes/users.rs --- */}
              <ModDirectory name="routes">
                <SourceFile path="users.rs" externalUses={["axum::Json"]}>
                  <FunctionDeclaration
                    name="list_users"
                    async
                    returns={<>Json&lt;Vec&lt;<Reference refkey={userKey} />&gt;&gt;</>}
                  >
                    todo!()
                  </FunctionDeclaration>
                  <AppendTo zone="router">
                    {"\n"}.route("/users", get(routes::users::list_users))
                  </AppendTo>
                </SourceFile>

                {/* --- Route depth 2: routes/orgs/posts.rs --- */}
                <ModDirectory name="orgs">
                  <SourceFile path="posts.rs" externalUses={["axum::Json", "axum::extract::Path"]}>
                    <FunctionDeclaration
                      name="list_org_posts"
                      async
                      params={[{ name: "Path(org_id)", type: "Path<i64>" }]}
                      returns={<>Json&lt;Vec&lt;<Reference refkey={postKey} />&gt;&gt;</>}
                    >
                      let _ = org_id;{"\n"}todo!()
                    </FunctionDeclaration>
                    <AppendTo zone="router">
                      {"\n"}.route("/orgs/:id/posts", get(routes::orgs::posts::list_org_posts))
                    </AppendTo>
                  </SourceFile>
                </ModDirectory>
              </ModDirectory>

              {/* --- lib.rs: mod declarations + router with zone --- */}
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

  it("lib.rs: mod declarations + router with zone-collected routes", () => {
    const res = buildTree();
    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod models;
      pub mod routes;

      use axum::Router;
      use axum::routing::get;

      pub fn router() -> Router {
        Router::new()
        .route("/users", get(routes::users::list_users))
        .route("/orgs/:id/posts", get(routes::orgs::posts::list_org_posts))
      }"
    `);
  });

  it("models: User and Post (Post references User)", () => {
    const res = buildTree();

    const user = findFile(res, "models/user.rs");
    expect(user.contents.trim()).toMatchInlineSnapshot(`
      "use serde::Deserialize;
      use serde::Serialize;

      #[derive(Debug, Clone, Serialize, Deserialize)]
      pub struct User {
        pub id: i64,
        pub name: String,
      }"
    `);

    const post = findFile(res, "models/post.rs");
    expect(post.contents.trim()).toMatchInlineSnapshot(`
      "use serde::Deserialize;
      use serde::Serialize;

      use crate::models::user::User;

      #[derive(Debug, Clone, Serialize, Deserialize)]
      pub struct Post {
        pub id: i64,
        pub title: String,
        pub author: User,
      }"
    `);
  });

  it("handler depth 1: routes/users.rs references User model", () => {
    const res = buildTree();
    const file = findFile(res, "routes/users.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;

      use crate::models::user::User;

      pub async fn list_users() -> Json<Vec<User>> {
        todo!()
      }"
    `);
  });

  it("handler depth 2: routes/orgs/posts.rs references Post model", () => {
    const res = buildTree();
    const file = findFile(res, "routes/orgs/posts.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;
      use axum::extract::Path;

      use crate::models::post::Post;

      pub async fn list_org_posts(Path(org_id): Path<i64>) -> Json<Vec<Post>> {
        let _ = org_id;
        todo!()
      }"
    `);
  });

  it("mod.rs files at each level", () => {
    const res = buildTree();

    const modelsMod = findFile(res, "models/mod.rs");
    expect(modelsMod.contents.trim()).toMatchInlineSnapshot(`
      "pub mod post;
      pub mod user;"
    `);

    const routesMod = findFile(res, "routes/mod.rs");
    expect(routesMod.contents.trim()).toMatchInlineSnapshot(`
      "pub mod orgs;
      pub mod users;"
    `);

    const orgsMod = findFile(res, "routes/orgs/mod.rs");
    expect(orgsMod.contents.trim()).toMatchInlineSnapshot(`
      "pub mod posts;"
    `);
  });

  it("compiles with cargo check, no unused vars", () => {
    const res = buildTree();

    rmSync(OUTPUT_DIR, { recursive: true, force: true });
    mkdirSync(join(OUTPUT_DIR, "src"), { recursive: true });

    writeFileSync(join(OUTPUT_DIR, "Cargo.toml"), `[package]
name = "axum-test"
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
