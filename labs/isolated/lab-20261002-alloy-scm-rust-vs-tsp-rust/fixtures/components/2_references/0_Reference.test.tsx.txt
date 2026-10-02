import { List, Output, render, refkey } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { CrateDirectory } from "../3_files/1_CrateDirectory.js";
import { SourceFile } from "../3_files/0_SourceFile.js";
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

describe("Reference", () => {
  it("same-file ref resolves to bare name, no use statement", () => {
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
    expect(file.contents.trim()).toMatchInlineSnapshot(`
      "struct Foo {}

      fn make_foo() -> Foo {
        todo!()
      }"
    `);
  });

  it("cross-file ref emits use statement", () => {
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
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod models;

      use crate::models::Foo;

      pub fn make_foo() -> Foo {
        todo!()
      }"
    `);
  });

  it("duplicate refs produce single use statement", () => {
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
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
      "pub mod models;

      use crate::models::Foo;

      pub fn make_foo() -> Foo {
        todo!()
      }

      pub fn another() -> Foo {
        todo!()
      }"
    `);
  });

  it("multiple cross-file refs produce sorted use statements", () => {
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
    expect(lib.contents.trim()).toMatchInlineSnapshot(`
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
  });
});
