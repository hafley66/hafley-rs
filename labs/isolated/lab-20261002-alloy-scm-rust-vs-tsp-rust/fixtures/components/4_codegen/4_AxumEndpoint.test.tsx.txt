import {
  List,
  Output,
  refkey,
  render,
} from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { VisibilityContext } from "../../scopes/06_contexts.js";
import { StructDeclaration, StructField } from "../1_declarations/0_StructDeclaration.js";
import { Reference } from "../2_references/0_Reference.js";
import { SourceFile } from "../3_files/0_SourceFile.js";
import { CrateDirectory } from "../3_files/1_CrateDirectory.js";
import { ModDirectory } from "../3_files/2_ModDirectory.js";
import { ZoneProvider, Zone } from "../4_codegen/0_AppendZone.js";
import { AxumEndpoint } from "./4_AxumEndpoint.js";
import { FunctionDeclaration } from "../1_declarations/4_FunctionDeclaration.js";

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

describe("AxumEndpoint", () => {
  const userKey = refkey();
  const postKey = refkey();

  function buildTree() {
    return render(
      <Output>
        <VisibilityContext.Provider value="pub">
          <ZoneProvider>
            <CrateDirectory>
              <SourceFile path="user.rs" externalUses={["serde::Deserialize", "serde::Serialize"]}>
                <StructDeclaration name="User" refkey={userKey} derive={["Debug", "Clone", "Serialize", "Deserialize"]}>
                  <List hardline>
                    <StructField name="id" type="i64" />
                    <StructField name="name" type="String" />
                  </List>
                </StructDeclaration>
              </SourceFile>

              {/* Simple endpoint: no params */}
              <AxumEndpoint
                path="/users"
                method="get"
                name="list_users"
                routePath="list_users"
                responseModel={userKey}
                responseList
              />

              {/* Nested endpoint: with Path extractor */}
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

                <AxumEndpoint
                  path="/orgs/:id/posts"
                  method="get"
                  name="list_org_posts"
                  routePath="orgs::list_org_posts"
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

  it("single file with auto handler zone + manual impl zone", () => {
    const res = buildTree();
    const file = findFile(res, "list_users.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;

      use crate::user::User;

      // alloy-handler-start
      pub async fn list_users() -> Json<Vec<User>> {
        Json(list_users_impl().await)
      }
      // alloy-handler-end

      pub async fn list_users_impl() -> Vec<User> {
        todo!()
      }"
    `);
  });

  it("parameterized endpoint: extractor in auto zone, plain param in manual zone", () => {
    const res = buildTree();
    const file = findFile(res, "orgs/list_org_posts.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;
      use axum::extract::Path;

      use crate::orgs::post::Post;

      // alloy-handler-start
      pub async fn list_org_posts(Path(org_id): Path<i64>) -> Json<Vec<Post>> {
        Json(list_org_posts_impl(org_id).await)
      }
      // alloy-handler-end

      pub async fn list_org_posts_impl(org_id: i64) -> Vec<Post> {
        let _ = org_id;
        todo!()
      }"
    `);
  });

  it("router collects route registrations from both endpoints", () => {
    const res = buildTree();
    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod list_users;
      pub mod orgs;
      pub mod user;

      use axum::Router;
      use axum::routing::get;

      pub fn router() -> Router {
        Router::new()
        .route("/users", get(list_users::list_users))
        .route("/orgs/:id/posts", get(orgs::list_org_posts::list_org_posts))
      }"
    `);
  });
});
