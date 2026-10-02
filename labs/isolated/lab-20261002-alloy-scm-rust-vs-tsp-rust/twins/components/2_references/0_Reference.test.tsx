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

describe("Reference", () => {
  it("same-file ref resolves to bare name, no use statement", () => probeTest(["d18b0ef1db802c7d"], () => {
    const fooKey = refkey();
    const res = render(
      <Output>
        <CrateDirectory>
          <SourceFile path="lib.rs">
            <StructDeclaration name="Foo" refkey={fooKey} braced />
            <hbr />
            <hbr />
            <FunctionDeclaration name="make_foo" returns={fooKey}>
              todo!()
            </FunctionDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const file = findFile(res, "lib.rs");
    probe("d18b0ef1db802c7d", () => (file.contents.trim()), `
      "struct Foo {}

      fn make_foo() -> Foo {
        todo!()
      }"
    `);
  }));

  it("cross-file ref emits use statement", () => probeTest(["110c8e7bd37f33a8"], () => {
    const fooKey = refkey();
    const res = render(
      <Output>
        <CrateDirectory>
          <SourceFile path="models.rs">
            <StructDeclaration name="Foo" refkey={fooKey} pub braced />
          </SourceFile>
          <SourceFile path="lib.rs">
            <FunctionDeclaration name="make_foo" pub returns={fooKey}>
              todo!()
            </FunctionDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const lib = findFile(res, "lib.rs");
    probe("110c8e7bd37f33a8", () => (lib.contents.trim()), `
      "pub mod models;

      use crate::models::Foo;

      pub fn make_foo() -> Foo {
        todo!()
      }"
    `);
  }));

  it("duplicate refs produce single use statement", () => probeTest(["e36c4527727d41a2"], () => {
    const fooKey = refkey();
    const res = render(
      <Output>
        <CrateDirectory>
          <SourceFile path="models.rs">
            <StructDeclaration name="Foo" refkey={fooKey} pub braced />
          </SourceFile>
          <SourceFile path="lib.rs">
            <FunctionDeclaration name="make_foo" pub returns={fooKey}>
              todo!()
            </FunctionDeclaration>
            <hbr />
            <hbr />
            <FunctionDeclaration name="another" pub returns={fooKey}>
              todo!()
            </FunctionDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const lib = findFile(res, "lib.rs");
    probe("e36c4527727d41a2", () => (lib.contents.trim()), `
      "pub mod models;

      use crate::models::Foo;

      pub fn make_foo() -> Foo {
        todo!()
      }

      pub fn another() -> Foo {
        todo!()
      }"
    `);
  }));

  it("multiple cross-file refs produce sorted use statements", () => probeTest(["391cec6700bc605f"], () => {
    const fooKey = refkey();
    const barKey = refkey();
    const res = render(
      <Output>
        <CrateDirectory>
          <SourceFile path="models.rs">
            <StructDeclaration name="Foo" refkey={fooKey} pub braced />
          </SourceFile>
          <SourceFile path="errors.rs">
            <EnumDeclaration name="AppError" refkey={barKey} pub>
              <List hardline>
                <UnitVariant name="NotFound" />
              </List>
            </EnumDeclaration>
          </SourceFile>
          <SourceFile path="lib.rs">
            <FunctionDeclaration name="get_foo" pub returns={fooKey}>
              todo!()
            </FunctionDeclaration>
            <hbr />
            <hbr />
            <FunctionDeclaration name="fail" pub returns={barKey}>
              todo!()
            </FunctionDeclaration>
          </SourceFile>
        </CrateDirectory>
      </Output>
    );

    const lib = findFile(res, "lib.rs");
    probe("391cec6700bc605f", () => (lib.contents.trim()), `
      "pub mod errors;
      pub mod models;

      use crate::errors::AppError;
      use crate::models::Foo;

      pub fn get_foo() -> Foo {
        todo!()
      }

      pub fn fail() -> AppError {
        todo!()
      }"
    `);
  }));
});
