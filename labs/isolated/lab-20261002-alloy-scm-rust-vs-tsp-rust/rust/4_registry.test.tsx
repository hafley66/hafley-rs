import { Output, refkey, render, useScope, type Children } from "@alloy-js/core";
import { expect, it } from "vitest";
import { CrateDirectory, SourceFile, ModDirectory, StructDeclaration, FunctionDeclaration } from "../2_components.js";
import { RustSourceFileScope } from "./1_scope.js";

function files(children: Children) {
  const result: Record<string, string> = {};
  function visit(node: ReturnType<typeof render>) {
    for (const item of node.contents) {
      if (item.kind === "file" && "contents" in item) result[item.path] = item.contents.trim();
      else if (item.kind === "directory") visit(item);
      else throw new Error("Unexpected copied file in registry render");
    }
  }
  visit(render(<Output><CrateDirectory>{children}</CrateDirectory></Output>));
  return result;
}

it("records declarations per file and resolves repeated forward refs through nested modules", () => {
  const user = refkey();
  const scopes: RustSourceFileScope[] = [];
  function Capture() {
    scopes.push(useScope() as RustSourceFileScope);
    return "";
  }
  const result = files(<>
    <SourceFile path="lib.rs">
      <Capture />
      <FunctionDeclaration name="first" returns={user}>todo!()</FunctionDeclaration>
      <hbr /><hbr />
      <FunctionDeclaration name="second" returns={user}>todo!()</FunctionDeclaration>
    </SourceFile>
    <ModDirectory name="models">
      <ModDirectory name="domain">
        <SourceFile path="user.rs"><Capture /><StructDeclaration name="User" refkey={user} pub braced /></SourceFile>
      </ModDirectory>
    </ModDirectory>
  </>);
  expect({files: result, records: scopes.map(scope => ({
    path: scope.modulePath,
    declarations: [...scope.declarations].map(symbol => symbol.name).sort(),
    uses: [...scope.uses.keys()].sort(),
  }))}).toMatchInlineSnapshot(`
    {
      "files": {
        "lib.rs": "pub mod models;

    use crate::models::domain::user::User;

    fn first() -> User {
      todo!()
    }

    fn second() -> User {
      todo!()
    }",
        "models/domain/mod.rs": "pub mod user;",
        "models/domain/user.rs": "pub struct User {}",
        "models/mod.rs": "pub mod domain;",
      },
      "records": [
        {
          "declarations": [
            "first",
            "second",
          ],
          "path": [],
          "uses": [
            "crate::models::domain::user::User",
          ],
        },
        {
          "declarations": [
            "User",
          ],
          "path": [
            "models",
            "domain",
            "user",
          ],
          "uses": [],
        },
      ],
    }
  `);
});

it("imports a crate-root declaration into a sibling file and keeps separate renders isolated", () => {
  const root = refkey();
  const first = files(<>
    <SourceFile path="worker.rs"><FunctionDeclaration name="run" returns={root}>todo!()</FunctionDeclaration></SourceFile>
    <SourceFile path="lib.rs"><StructDeclaration name="Root" refkey={root} pub braced /></SourceFile>
  </>);
  const second = files(<SourceFile path="lib.rs"><StructDeclaration name="Standalone" braced /></SourceFile>);
  expect({first, second}).toMatchInlineSnapshot(`
    {
      "first": {
        "lib.rs": "pub mod worker;

    pub struct Root {}",
        "worker.rs": "use crate::Root;

    fn run() -> Root {
      todo!()
    }",
      },
      "second": {
        "lib.rs": "struct Standalone {}",
      },
    }
  `);
});
