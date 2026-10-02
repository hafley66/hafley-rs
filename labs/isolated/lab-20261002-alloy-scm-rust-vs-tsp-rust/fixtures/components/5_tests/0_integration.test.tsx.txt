import { List, Output, render, refkey } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { CrateDirectory } from "../3_files/1_CrateDirectory.js";
import { SourceFile } from "../3_files/0_SourceFile.js";
import { ModDirectory } from "../3_files/2_ModDirectory.js";
import { StructDeclaration, StructField } from "../1_declarations/0_StructDeclaration.js";
import { FunctionDeclaration } from "../1_declarations/4_FunctionDeclaration.js";
import { EnumDeclaration, UnitVariant } from "../1_declarations/1_EnumDeclaration.js";
import { ImplBlock } from "../1_declarations/6_ImplBlock.js";

function findFile(res: any, path: string): any {
  for (const item of res.contents) {
    if (item.kind === "file" && item.path === path) return item;
    if (item.kind === "directory") {
      const found = findFile(item, path);
      if (found) return found;
    }
  }
  return null;
}

describe("externalUses", () => {
  it("renders external use statements sorted before crate uses", () => {
    const userKey = refkey();
    const res = render(
      <Output>
        <CrateDirectory>
          <ModDirectory name="models">
            <SourceFile path="user.rs">
              <StructDeclaration name="User" refkey={userKey} pub braced />
            </SourceFile>
          </ModDirectory>
          <SourceFile
            path="lib.rs"
            externalUses={["axum::extract::Json", "axum::routing::get"]}
          >
            <FunctionDeclaration name="handler" pub async returns="Json<User>">
              todo!()
            </FunctionDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod models;

      use axum::extract::Json;
      use axum::routing::get;

      pub async fn handler() -> Json<User> {
        todo!()
      }"
    `);
  });

  it("external uses without crate uses", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <SourceFile
            path="main.rs"
          >
            <FunctionDeclaration name="main" attrs={["tokio::main"]} pub async>
              println!("hello")
            </FunctionDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const main = findFile(res, "main.rs");
    expect(main.contents.trim()).toMatchInlineSnapshot(`
      "#[tokio::main]
      pub async fn main() {
        println!("hello")
      }"
    `);
  });
});

describe("attributes", () => {
  it("attrs on struct", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <SourceFile path="lib.rs">
            <StructDeclaration
              name="User"
              pub
              derive={["Debug", "Clone", "Serialize", "Deserialize"]}
              attrs={['serde(rename_all = "camelCase")']}
            >
              <List hardline>
                <StructField name="id" type="i64" pub />
                <StructField name="user_name" type="String" pub attrs={['serde(rename = "username")']} />
              </List>
            </StructDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "#[derive(Debug, Clone, Serialize, Deserialize)]
      #[serde(rename_all = "camelCase")]
      pub struct User {
        pub id: i64,
        #[serde(rename = "username")]
        pub user_name: String,
      }"
    `);
  });

  it("attrs on function", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <SourceFile path="lib.rs">
            <FunctionDeclaration name="index" pub async attrs={['get("/")']} returns="&'static str">
              "hello"
            </FunctionDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "#[get("/")]
      pub async fn index() -> &'static str {
        "hello"
      }"
    `);
  });

  it("attrs on enum", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <SourceFile path="lib.rs">
            <EnumDeclaration
              name="Status"
              pub
              derive={["Debug", "Serialize"]}
              attrs={['serde(rename_all = "lowercase")']}
            >
              <List hardline>
                <UnitVariant name="Active" />
                <UnitVariant name="Inactive" />
              </List>
            </EnumDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "#[derive(Debug, Serialize)]
      #[serde(rename_all = "lowercase")]
      pub enum Status {
        Active,
        Inactive,
      }"
    `);
  });
});

describe("axum integration", () => {
  it("generates a complete axum handler crate", () => {
    const userKey = refkey();
    const appErrorKey = refkey();

    const res = render(
      <Output>
        <CrateDirectory>
          <ModDirectory name="models">
            <SourceFile path="user.rs" externalUses={["serde::Serialize", "serde::Deserialize", "sqlx::FromRow"]}>
              <StructDeclaration
                name="User"
                refkey={userKey}
                pub
                derive={["Debug", "Clone", "Serialize", "Deserialize", "FromRow"]}
              >
                <List hardline>
                  <StructField name="id" type="i64" pub />
                  <StructField name="name" type="String" pub />
                  <StructField name="email" type="String" pub />
                </List>
              </StructDeclaration>
            </SourceFile>
          </ModDirectory>
          <ModDirectory name="errors">
            <SourceFile path="error.rs">
              <EnumDeclaration name="AppError" refkey={appErrorKey} pub derive={["Debug"]}>
                <List hardline>
                  <UnitVariant name="NotFound" />
                  <UnitVariant name="InternalError" />
                </List>
              </EnumDeclaration>
            </SourceFile>
          </ModDirectory>
          <ModDirectory name="routes">
            <SourceFile
              path="users.rs"
              externalUses={[
                "axum::extract::State",
                "axum::Json",
                "sqlx::SqlitePool",
              ]}
            >
              <FunctionDeclaration
                name="list_users"
                pub
                async
                params={[{ name: "State(pool)", type: "State<SqlitePool>" }]}
                returns={<>Result&lt;Json&lt;Vec&lt;{userKey}&gt;&gt;, {appErrorKey}&gt;</>}
              >
                todo!()
              </FunctionDeclaration>
            </SourceFile>
          </ModDirectory>
          <SourceFile path="lib.rs" />
        </CrateDirectory>
      </Output>
    );

    // models/user.rs
    const userFile = findFile(res, "models/user.rs");
    expect(userFile.contents.trim()).toMatchInlineSnapshot(`
      "use serde::Deserialize;
      use serde::Serialize;
      use sqlx::FromRow;

      #[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
      pub struct User {
        pub id: i64,
        pub name: String,
        pub email: String,
      }"
    `);

    // models/mod.rs
    const modelsMod = findFile(res, "models/mod.rs");
    expect(modelsMod.contents.trim()).toMatchInlineSnapshot(`"pub mod user;"`);

    // errors/error.rs
    const errorFile = findFile(res, "errors/error.rs");
    expect(errorFile.contents.trim()).toMatchInlineSnapshot(`
      "#[derive(Debug)]
      pub enum AppError {
        NotFound,
        InternalError,
      }"
    `);

    // routes/users.rs -- cross-file refs to User and AppError
    const routesFile = findFile(res, "routes/users.rs");
    expect(routesFile.contents.trim()).toMatchInlineSnapshot(`
      "use axum::Json;
      use axum::extract::State;
      use sqlx::SqlitePool;

      use crate::errors::error::AppError;
      use crate::models::user::User;

      pub async fn list_users(State(pool): State<SqlitePool>) -> Result<Json<Vec<User>>, AppError> {
        todo!()
      }"
    `);

    // lib.rs -- mod declarations for all three directories
    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod errors;
      pub mod models;
      pub mod routes;"
    `);
  });
});
