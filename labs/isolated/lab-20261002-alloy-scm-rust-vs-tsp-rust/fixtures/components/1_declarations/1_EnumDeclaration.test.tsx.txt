import { List, Output, Scope, createScope } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { RustLexicalScope } from "../../scopes/01_lexical.js";
import { StructField } from "./0_StructDeclaration.js";
import {
  EnumDeclaration,
  UnitVariant,
  TupleVariant,
  StructVariant,
} from "./1_EnumDeclaration.js";

function RustRoot(props: { children: any }) {
  const scope = createScope(RustLexicalScope, "root", undefined);
  return (
    <Output>
      <Scope value={scope}>
        {props.children}
      </Scope>
    </Output>
  );
}

describe("EnumDeclaration", () => {
  it("unit variants", () => {
    expect(
      <RustRoot>
        <EnumDeclaration name="Color">
          <List hardline>
            <UnitVariant name="Red" />
            <UnitVariant name="Green" />
            <UnitVariant name="Blue" />
          </List>
        </EnumDeclaration>
      </RustRoot>
    ).toRenderTo(`
      enum Color {
        Red,
        Green,
        Blue,
      }
    `);
  });

  it("tuple variants", () => {
    expect(
      <RustRoot>
        <EnumDeclaration name="Shape">
          <List hardline>
            <TupleVariant name="Circle" fields={["f64"]} />
            <TupleVariant name="Rect" fields={["f64", "f64"]} />
          </List>
        </EnumDeclaration>
      </RustRoot>
    ).toRenderTo(`
      enum Shape {
        Circle(f64),
        Rect(f64, f64),
      }
    `);
  });

  it("struct variants", () => {
    expect(
      <RustRoot>
        <EnumDeclaration name="Message">
          <List hardline>
            <UnitVariant name="Quit" />
            <StructVariant name="Move">
              <List hardline>
                <StructField name="x" type="i32" />
                <StructField name="y" type="i32" />
              </List>
            </StructVariant>
          </List>
        </EnumDeclaration>
      </RustRoot>
    ).toRenderTo(`
      enum Message {
        Quit,
        Move {
          x: i32,
          y: i32,
        },
      }
    `);
  });

  it("mixed variant types", () => {
    expect(
      <RustRoot>
        <EnumDeclaration name="Event">
          <List hardline>
            <UnitVariant name="None" />
            <TupleVariant name="Click" fields={["i32", "i32"]} />
            <StructVariant name="KeyPress">
              <List hardline>
                <StructField name="key" type="char" />
                <StructField name="ctrl" type="bool" />
              </List>
            </StructVariant>
          </List>
        </EnumDeclaration>
      </RustRoot>
    ).toRenderTo(`
      enum Event {
        None,
        Click(i32, i32),
        KeyPress {
          key: char,
          ctrl: bool,
        },
      }
    `);
  });

  it("derive attribute", () => {
    expect(
      <RustRoot>
        <EnumDeclaration name="Color" derive={["Debug", "PartialEq"]}>
          <List hardline>
            <UnitVariant name="Red" />
          </List>
        </EnumDeclaration>
      </RustRoot>
    ).toRenderTo(`
      #[derive(Debug, PartialEq)]
      enum Color {
        Red,
      }
    `);
  });

  it("generic enum", () => {
    expect(
      <RustRoot>
        <EnumDeclaration name="Option" typeParams={[{ name: "T" }]}>
          <List hardline>
            <TupleVariant name="Some" fields={["T"]} />
            <UnitVariant name="None" />
          </List>
        </EnumDeclaration>
      </RustRoot>
    ).toRenderTo(`
      enum Option<T> {
        Some(T),
        None,
      }
    `);
  });

  it("pub enum", () => {
    expect(
      <RustRoot>
        <EnumDeclaration name="Color" pub>
          <List hardline>
            <UnitVariant name="Red" />
          </List>
        </EnumDeclaration>
      </RustRoot>
    ).toRenderTo(`
      pub enum Color {
        Red,
      }
    `);
  });
});
