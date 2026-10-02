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

const OUTPUT_DIR = join(import.meta.dirname, "../../test-output-colocated");

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

describe("colocated model placement", () => {
  const userKey = refkey();
  const postKey = refkey();

  // User: referenced by users.rs (root) and Post (orgs/) → hoists to root
  // Post: only referenced by orgs/posts.rs → stays in orgs/
  //
  // src/
  //   lib.rs          ← router + mod user; mod users; mod orgs;
  //   user.rs         ← User model (hoisted)
  //   users.rs        ← GET /users
  //   orgs/
  //     mod.rs        ← mod post; mod posts;
  //     post.rs       ← Post model (colocated)
  //     posts.rs      ← GET /orgs/:id/posts
  function buildTree() {
    return render(
      <Output>
        <VisibilityContext.Provider value="pub">
          <ZoneProvider>
            <CrateDirectory>
              {/* User model: hoisted to crate root (consumed at root + orgs/) */}
              <SourceFile path="user.rs" externalUses={["serde::Deserialize", "serde::Serialize"]}>
                <StructDeclaration name="User" refkey={userKey} derive={["Debug", "Clone", "Serialize", "Deserialize"]}>
                  <List hardline>
                    <StructField name="id" type="i64" />
                    <StructField name="name" type="String" />
                  </List>
                </StructDeclaration>
              </SourceFile>

              {/* Handler: GET /users (root level, refs User from sibling) */}
              <SourceFile path="users.rs" externalUses={["axum::Json"]}>
                <FunctionDeclaration
                  name="list_users"
                  async
                  returns={<>Json&lt;Vec&lt;<Reference refkey={userKey} />&gt;&gt;</>}
                >
                  todo!()
                </FunctionDeclaration>
                <AppendTo zone="router">
                  {"\n"}.route("/users", get(users::list_users))
                </AppendTo>
              </SourceFile>

              {/* Orgs domain: model + handler colocated */}
              <ModDirectory name="orgs">
                {/* Post model: stays here (only consumed within orgs/) */}
                <SourceFile path="post.rs" externalUses={["serde::Deserialize", "serde::Serialize"]}>
                  <StructDeclaration name="Post" refkey={postKey} derive={["Debug", "Clone", "Serialize", "Deserialize"]}>
                    <List hardline>
                      <StructField name="id" type="i64" />
                      <StructField name="title" type="String" />
                      <StructField name="author" type={<Reference refkey={userKey} />} />
                    </List>
                  </StructDeclaration>
                </SourceFile>

                {/* Handler: GET /orgs/:id/posts (refs Post from sibling, User from parent) */}
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
                    {"\n"}.route("/orgs/:id/posts", get(orgs::posts::list_org_posts))
                  </AppendTo>
                </SourceFile>
              </ModDirectory>

              {/* lib.rs: mod declarations auto-generated, router via zone */}
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

  it("lib.rs: flat mod declarations + router", () => {
    const res = buildTree();
    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod orgs;
      pub mod user;
      pub mod users;

      use axum::Router;
      use axum::routing::get;

      pub fn router() -> Router {
        Router::new()
        .route("/users", get(users::list_users))
        .route("/orgs/:id/posts", get(orgs::posts::list_org_posts))
      }"
    `);
  });

  it("user.rs at crate root (hoisted)", () => {
    const res = buildTree();
    const file = findFile(res, "user.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use serde::Deserialize;
      use serde::Serialize;

      #[derive(Debug, Clone, Serialize, Deserialize)]
      pub struct User {
        pub id: i64,
        pub name: String,
      }"
    `);
  });

  it("post.rs colocated in orgs/, references User from parent", () => {
    const res = buildTree();
    const file = findFile(res, "orgs/post.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use serde::Deserialize;
      use serde::Serialize;

      use crate::user::User;

      #[derive(Debug, Clone, Serialize, Deserialize)]
      pub struct Post {
        pub id: i64,
        pub title: String,
        pub author: User,
      }"
    `);
  });

  it("users.rs handler at root, refs sibling user module", () => {
    const res = buildTree();
    const file = findFile(res, "users.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;

      use crate::user::User;

      pub async fn list_users() -> Json<Vec<User>> {
        todo!()
      }"
    `);
  });

  it("orgs/posts.rs handler refs colocated Post", () => {
    const res = buildTree();
    const file = findFile(res, "routes/orgs/posts.rs");
    expect(file).toBeNull(); // NOT in routes/ -- colocated

    const actual = findFile(res, "orgs/posts.rs");
    expect(actual.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;
      use axum::extract::Path;

      use crate::orgs::post::Post;

      pub async fn list_org_posts(Path(org_id): Path<i64>) -> Json<Vec<Post>> {
        let _ = org_id;
        todo!()
      }"
    `);
  });

  it("orgs/mod.rs declares colocated children", () => {
    const res = buildTree();
    const mod_ = findFile(res, "orgs/mod.rs");
    expect(mod_.contents.trim()).toMatchInlineSnapshot(`
      "pub mod post;
      pub mod posts;"
    `);
  });

  it("compiles with cargo check, no unused vars", () => {
    const res = buildTree();

    rmSync(OUTPUT_DIR, { recursive: true, force: true });
    mkdirSync(join(OUTPUT_DIR, "src"), { recursive: true });

    writeFileSync(join(OUTPUT_DIR, "Cargo.toml"), `[package]
name = "colocated-test"
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
