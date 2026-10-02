import { probe, probeTest } from "../../../4_probe.js";
import { List, Output, Scope, createScope } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { RustLexicalScope } from "../../../2_components.js";
import { StructField } from "../../../2_components.js";
import {
  EnumDeclaration,
  UnitVariant,
  TupleVariant,
  StructVariant,
} from "../../../2_components.js";

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
  it("unit variants", () => probeTest(["4b9ffa183e24d104"], () => {
    probe("4b9ffa183e24d104", () => (<RustRoot>
        <EnumDeclaration name="Color">
          <List hardline>
            <UnitVariant name="Red" />
            <UnitVariant name="Green" />
            <UnitVariant name="Blue" />
          </List>
        </EnumDeclaration>
      </RustRoot>), `
      enum Color {
        Red,
        Green,
        Blue,
      }
    `);
  }));

  it("tuple variants", () => probeTest(["a2d19f4d7a6c2628"], () => {
    probe("a2d19f4d7a6c2628", () => (<RustRoot>
        <EnumDeclaration name="Shape">
          <List hardline>
            <TupleVariant name="Circle" fields={["f64"]} />
            <TupleVariant name="Rect" fields={["f64", "f64"]} />
          </List>
        </EnumDeclaration>
      </RustRoot>), `
      enum Shape {
        Circle(f64),
        Rect(f64, f64),
      }
    `);
  }));

  it("struct variants", () => probeTest(["4ea6eb9d047435f9"], () => {
    probe("4ea6eb9d047435f9", () => (<RustRoot>
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
      </RustRoot>), `
      enum Message {
        Quit,
        Move {
          x: i32,
          y: i32,
        },
      }
    `);
  }));

  it("mixed variant types", () => probeTest(["c2abbb2e49c0bd78"], () => {
    probe("c2abbb2e49c0bd78", () => (<RustRoot>
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
      </RustRoot>), `
      enum Event {
        None,
        Click(i32, i32),
        KeyPress {
          key: char,
          ctrl: bool,
        },
      }
    `);
  }));

  it("derive attribute", () => probeTest(["d48cea47d1947295"], () => {
    probe("d48cea47d1947295", () => (<RustRoot>
        <EnumDeclaration name="Color" derive={["Debug", "PartialEq"]}>
          <List hardline>
            <UnitVariant name="Red" />
          </List>
        </EnumDeclaration>
      </RustRoot>), `
      #[derive(Debug, PartialEq)]
      enum Color {
        Red,
      }
    `);
  }));

  it("generic enum", () => probeTest(["be264f149cb2c0ec"], () => {
    probe("be264f149cb2c0ec", () => (<RustRoot>
        <EnumDeclaration name="Option" typeParams={[{ name: "T" }]}>
          <List hardline>
            <TupleVariant name="Some" fields={["T"]} />
            <UnitVariant name="None" />
          </List>
        </EnumDeclaration>
      </RustRoot>), `
      enum Option<T> {
        Some(T),
        None,
      }
    `);
  }));

  it("pub enum", () => probeTest(["23ab8d3d2622a202"], () => {
    probe("23ab8d3d2622a202", () => (<RustRoot>
        <EnumDeclaration name="Color" pub>
          <List hardline>
            <UnitVariant name="Red" />
          </List>
        </EnumDeclaration>
      </RustRoot>), `
      pub enum Color {
        Red,
      }
    `);
  }));
});
