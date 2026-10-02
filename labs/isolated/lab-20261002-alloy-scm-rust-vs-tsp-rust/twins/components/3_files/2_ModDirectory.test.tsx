import { probe, probeTest, render } from "../../../4_probe.js";
import { List, Output, refkey } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { CrateDirectory } from "../../../2_components.js";
import { SourceFile } from "../../../2_components.js";
import { ModDirectory } from "../../../2_components.js";
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

describe("ModDirectory", () => {
  it("generates mod.rs with child module declarations", () => probeTest(["c44ac5aa2e22f495","e9a3f730030af960"], () => {
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
    probe("c44ac5aa2e22f495", () => (modRs), null);
    probe("e9a3f730030af960", () => (modRs.contents.trim()), `
      "pub mod order;
      pub mod user;"
    `);
  }));

  it("lib.rs declares top-level modules", () => probeTest(["40e06abe8f3645b1"], () => {
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
    probe("40e06abe8f3645b1", () => (lib.contents.trim()), `
      "pub mod models;

      pub fn run() {
        todo!()
      }"
    `);
  }));

  it("cross-file ref through nested module generates correct use path", () => probeTest(["679aabc06da58d41"], () => {
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
    probe("679aabc06da58d41", () => (lib.contents.trim()), `
      "pub mod models;

      use crate::models::user::User;

      pub fn get_user() -> User {
        todo!()
      }"
    `);
  }));

  it("nested mod directories produce deep use paths", () => probeTest(["7196197f6ae131e2","ec47ade7ed27b46b","eb5427deb7d1ff63","c455311b9cbfbb49","34b3ca405bad50fa"], () => {
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
    probe("7196197f6ae131e2", () => (lib.contents.trim()), `
      "pub mod a;

      use crate::a::b::thing::Thing;

      pub fn get_thing() -> Thing {
        todo!()
      }"
    `);

    const aMod = findFile(res, "a/mod.rs");
    probe("ec47ade7ed27b46b", () => (aMod), null);
    probe("eb5427deb7d1ff63", () => (aMod.contents.trim()), `"pub mod b;"`);

    const bMod = findFile(res, "a/b/mod.rs");
    probe("c455311b9cbfbb49", () => (bMod), null);
    probe("34b3ca405bad50fa", () => (bMod.contents.trim()), `"pub mod thing;"`);
  }));

  it("flat sibling files register as modules in lib.rs", () => probeTest(["e11da2044d586e68"], () => {
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
    probe("e11da2044d586e68", () => (lib.contents.trim()), `"pub mod utils;"`);
  }));
});
