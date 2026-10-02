import { probe, probeTest } from "../../../4_probe.js";
import { List, Output, render, refkey } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { CrateDirectory } from "../../../2_components.js";
import { SourceFile } from "../../../2_components.js";
import { StructDeclaration, StructField } from "../../../2_components.js";
import { FunctionDeclaration } from "../../../2_components.js";
import { EnumDeclaration, UnitVariant } from "../../../2_components.js";

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
  it("renders a single file with a struct", () => probeTest(["22806a091a6df2b0","31b371ea6a954181"], () => {
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
    probe("22806a091a6df2b0", () => (file), null);
    probe("31b371ea6a954181", () => (file.contents.trim()), `
      "pub struct Foo {
        pub x: i32,
      }"
    `);
  }));

  it("renders multiple declarations in one file", () => probeTest(["4d8ee70912291f25"], () => {
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
    probe("4d8ee70912291f25", () => (file.contents.trim()), `
      "#[derive(Debug)]
      pub struct Point {
        pub x: f64,
        pub y: f64,
      }

      pub fn origin() -> Point {
        todo!()
      }"
    `);
  }));

  it("renders two source files in the same crate", () => probeTest(["e340fe9415eb8c3f","c54a875baa649c3c"], () => {
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
    probe("e340fe9415eb8c3f", () => (models.contents.trim()), `"pub struct User {}"`);

    const lib = findFile(res, "lib.rs");
    probe("c54a875baa649c3c", () => (lib.contents.trim()), `
      "pub mod models;

      fn main() {
        todo!()
      }"
    `);
  }));
});
