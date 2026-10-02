import { List, Output, render, refkey } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { CrateDirectory } from "./1_CrateDirectory.js";
import { SourceFile } from "./0_SourceFile.js";
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

describe("SourceFile", () => {
  it("renders a single file with a struct", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <SourceFile path="lib.rs">
            <StructDeclaration name="Foo" pub>
              <List hardline>
                <StructField name="x" type="i32" pub />
              </List>
            </StructDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const file = findFile(res, "lib.rs");
    expect(file).not.toBeNull();
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "pub struct Foo {
        pub x: i32,
      }"
    `);
  });

  it("renders multiple declarations in one file", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <SourceFile path="lib.rs">
            <StructDeclaration name="Point" pub derive={["Debug"]}>
              <List hardline>
                <StructField name="x" type="f64" pub />
                <StructField name="y" type="f64" pub />
              </List>
            </StructDeclaration>
            <hbr />
            <hbr />
            <FunctionDeclaration name="origin" pub returns="Point">
              todo!()
            </FunctionDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const file = findFile(res, "lib.rs");
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "#[derive(Debug)]
      pub struct Point {
        pub x: f64,
        pub y: f64,
      }

      pub fn origin() -> Point {
        todo!()
      }"
    `);
  });

  it("renders two source files in the same crate", () => {
    const res = render(
      <Output>
        <CrateDirectory>
          <SourceFile path="models.rs">
            <StructDeclaration name="User" pub braced />
          </SourceFile>
          <SourceFile path="lib.rs">
            <FunctionDeclaration name="main">
              todo!()
            </FunctionDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const models = findFile(res, "models.rs");
    expect(models.contents.trim()).toMatchInlineSnapshot(`"pub struct User {}"`);

    const lib = findFile(res, "lib.rs");
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod models;

      fn main() {
        todo!()
      }"
    `);
  });
});
