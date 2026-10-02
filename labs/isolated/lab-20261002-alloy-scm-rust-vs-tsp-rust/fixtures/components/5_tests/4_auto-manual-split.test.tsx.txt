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

const OUTPUT_DIR = join(import.meta.dirname, "../../test-output-auto-manual");

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

describe("auto/manual split", () => {
  const userKey = refkey();
  const postKey = refkey();

  // Layout:
  //   src/
  //     lib.rs              ← router + mod declarations
  //     user.rs             ← User model (hoisted)
  //     users_auto.rs       ← AUTO: axum handler, delegates to users::list_users_impl
  //     users.rs            ← MANUAL stub: pure domain fn
  //     orgs/
  //       mod.rs
  //       post.rs           ← Post model (colocated)
  //       posts_auto.rs     ← AUTO: axum handler, delegates to posts::list_org_posts_impl
  //       posts.rs          ← MANUAL stub: pure domain fn

  function buildTree() {
    return render(
      <Output>
        <VisibilityContext.Provider value="pub">
          <ZoneProvider>
            <CrateDirectory>
              {/* --- Models --- */}
              <SourceFile path="user.rs" externalUses={["serde::Deserialize", "serde::Serialize"]}>
                <StructDeclaration name="User" refkey={userKey} derive={["Debug", "Clone", "Serialize", "Deserialize"]}>
                  <List hardline>
                    <StructField name="id" type="i64" />
                    <StructField name="name" type="String" />
                  </List>
                </StructDeclaration>
              </SourceFile>

              {/* --- Endpoint 1 (depth 1): /users --- */}

              {/* AUTO: axum handler wrapper */}
              <SourceFile path="users_auto.rs" externalUses={["axum::Json"]}>
                <FunctionDeclaration
                  name="list_users"
                  async
                  returns={<>Json&lt;Vec&lt;<Reference refkey={userKey} />&gt;&gt;</>}
                >
                  Json(crate::users::list_users_impl().await)
                </FunctionDeclaration>
                <AppendTo zone="router">
                  {"\n"}.route("/users", get(users_auto::list_users))
                </AppendTo>
              </SourceFile>

              {/* MANUAL: stub, user fills in */}
              <SourceFile path="users.rs">
                <FunctionDeclaration
                  name="list_users_impl"
                  async
                  returns={<>Vec&lt;<Reference refkey={userKey} />&gt;</>}
                >
                  todo!()
                </FunctionDeclaration>
              </SourceFile>

              {/* --- Endpoint 2 (depth 2): /orgs/:id/posts --- */}
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

                {/* AUTO: axum handler wrapper */}
                <SourceFile path="posts_auto.rs" externalUses={["axum::Json", "axum::extract::Path"]}>
                  <FunctionDeclaration
                    name="list_org_posts"
                    async
                    params={[{ name: "Path(org_id)", type: "Path<i64>" }]}
                    returns={<>Json&lt;Vec&lt;<Reference refkey={postKey} />&gt;&gt;</>}
                  >
                    Json(crate::orgs::posts::list_org_posts_impl(org_id).await)
                  </FunctionDeclaration>
                  <AppendTo zone="router">
                    {"\n"}.route("/orgs/:id/posts", get(orgs::posts_auto::list_org_posts))
                  </AppendTo>
                </SourceFile>

                {/* MANUAL: stub, user fills in */}
                <SourceFile path="posts.rs">
                  <FunctionDeclaration
                    name="list_org_posts_impl"
                    async
                    params={[{ name: "org_id", type: "i64" }]}
                    returns={<>Vec&lt;<Reference refkey={postKey} />&gt;</>}
                  >
                    let _ = org_id;{"\n"}todo!()
                  </FunctionDeclaration>
                </SourceFile>
              </ModDirectory>

              {/* --- lib.rs: router via zone --- */}
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

  it("users_auto.rs: axum wrapper delegates to manual impl", () => {
    const res = buildTree();
    const file = findFile(res, "users_auto.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;

      use crate::user::User;

      pub async fn list_users() -> Json<Vec<User>> {
        Json(crate::users::list_users_impl().await)
      }"
    `);
  });

  it("users.rs: manual stub with pure domain signature", () => {
    const res = buildTree();
    const file = findFile(res, "users.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use crate::user::User;

      pub async fn list_users_impl() -> Vec<User> {
        todo!()
      }"
    `);
  });

  it("orgs/posts_auto.rs: deeper auto handler with extractor", () => {
    const res = buildTree();
    const file = findFile(res, "orgs/posts_auto.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;
      use axum::extract::Path;

      use crate::orgs::post::Post;

      pub async fn list_org_posts(Path(org_id): Path<i64>) -> Json<Vec<Post>> {
        Json(crate::orgs::posts::list_org_posts_impl(org_id).await)
      }"
    `);
  });

  it("orgs/posts.rs: manual stub, no framework imports", () => {
    const res = buildTree();
    const file = findFile(res, "orgs/posts.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use crate::orgs::post::Post;

      pub async fn list_org_posts_impl(org_id: i64) -> Vec<Post> {
        let _ = org_id;
        todo!()
      }"
    `);
  });

  it("lib.rs: router collects auto handlers", () => {
    const res = buildTree();
    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod orgs;
      pub mod user;
      pub mod users;
      pub mod users_auto;

      use axum::Router;
      use axum::routing::get;

      pub fn router() -> Router {
        Router::new()
        .route("/users", get(users_auto::list_users))
        .route("/orgs/:id/posts", get(orgs::posts_auto::list_org_posts))
      }"
    `);
  });

  it("mod.rs declares both auto and manual modules", () => {
    const res = buildTree();
    const mod_ = findFile(res, "orgs/mod.rs");
    expect(mod_.contents.trim()).toMatchInlineSnapshot(`
      "pub mod post;
      pub mod posts;
      pub mod posts_auto;"
    `);
  });

  it("compiles with cargo check, no unused vars", () => {
    const res = buildTree();

    rmSync(OUTPUT_DIR, { recursive: true, force: true });
    mkdirSync(join(OUTPUT_DIR, "src"), { recursive: true });

    writeFileSync(join(OUTPUT_DIR, "Cargo.toml"), `[package]
name = "auto-manual-test"
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
