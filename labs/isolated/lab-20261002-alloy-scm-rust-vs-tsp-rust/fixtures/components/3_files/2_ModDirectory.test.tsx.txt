import { List, Output, render, refkey } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { CrateDirectory } from "./1_CrateDirectory.js";
import { SourceFile } from "./0_SourceFile.js";
import { ModDirectory } from "./2_ModDirectory.js";
import { StructDeclaration, StructField } from "../1_declarations/0_StructDeclaration.js";
import { FunctionDeclaration } from "../1_declarations/4_FunctionDeclaration.js";
import { EnumDeclaration, UnitVariant } from "../1_declarations/1_EnumDeclaration.js";

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

describe("ModDirectory", () => {
  it("generates mod.rs with child module declarations", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <ModDirectory name="models">
            <SourceFile path="user.rs">
              <StructDeclaration name="User" pub braced />
            </SourceFile>
            <SourceFile path="order.rs">
              <StructDeclaration name="Order" pub braced />
            </SourceFile>
          </ModDirectory>
          <SourceFile path="lib.rs" />
        </CrateDirectory>
      </Output>
    );

    const modRs = findFile(res, "models/mod.rs");
    expect(modRs).not.toBeNull();
    expect(modRs.contents.trim()).toMatchInlineSnapshot(`
      "pub mod order;
      pub mod user;"
    `);
  });

  it("lib.rs declares top-level modules", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <ModDirectory name="models">
            <SourceFile path="user.rs">
              <StructDeclaration name="User" pub braced />
            </SourceFile>
          </ModDirectory>
          <SourceFile path="lib.rs">
            <FunctionDeclaration name="run" pub>
              todo!()
            </FunctionDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod models;

      pub fn run() {
        todo!()
      }"
    `);
  });

  it("cross-file ref through nested module generates correct use path", () => {
    const userKey = refkey();
    const res = render(
      <Output>
        <CrateDirectory>
          <ModDirectory name="models">
            <SourceFile path="user.rs">
              <StructDeclaration name="User" refkey={userKey} pub braced />
            </SourceFile>
          </ModDirectory>
          <SourceFile path="lib.rs">
            <FunctionDeclaration name="get_user" pub returns={userKey}>
              todo!()
            </FunctionDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod models;

      use crate::models::user::User;

      pub fn get_user() -> User {
        todo!()
      }"
    `);
  });

  it("nested mod directories produce deep use paths", () => {
    const thingKey = refkey();
    const res = render(
      <Output>
        <CrateDirectory>
          <ModDirectory name="a">
            <ModDirectory name="b">
              <SourceFile path="thing.rs">
                <StructDeclaration name="Thing" refkey={thingKey} pub braced />
              </SourceFile>
            </ModDirectory>
          </ModDirectory>
          <SourceFile path="lib.rs">
            <FunctionDeclaration name="get_thing" pub returns={thingKey}>
              todo!()
            </FunctionDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod a;

      use crate::a::b::thing::Thing;

      pub fn get_thing() -> Thing {
        todo!()
      }"
    `);

    const aMod = findFile(res, "a/mod.rs");
    expect(aMod).not.toBeNull();
    expect(aMod.contents.trim()).toMatchInlineSnapshot(`"pub mod b;"`);

    const bMod = findFile(res, "a/b/mod.rs");
    expect(bMod).not.toBeNull();
    expect(bMod.contents.trim()).toMatchInlineSnapshot(`"pub mod thing;"`);
  });

  it("flat sibling files register as modules in lib.rs", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <SourceFile path="utils.rs">
            <FunctionDeclaration name="helper" pub>
              42
            </FunctionDeclaration>
          </SourceFile>
          <SourceFile path="lib.rs" />
        </CrateDirectory>
      </Output>
    );

    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`"pub mod utils;"`);
  });
});
