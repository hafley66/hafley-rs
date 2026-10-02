import { List, Output, Scope, createScope } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { RustLexicalScope } from "../../scopes/01_lexical.js";
import { StructDeclaration, StructField, TupleStructDeclaration } from "./0_StructDeclaration.js";

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

describe("StructDeclaration", () => {
  it("unit struct (no body)", () => {
    expect(
      <RustRoot><StructDeclaration name="Foo" /></RustRoot>
    ).toRenderTo("struct Foo;");
  });

  it("empty braced struct", () => {
    expect(
      <RustRoot><StructDeclaration name="Foo" braced /></RustRoot>
    ).toRenderTo("struct Foo {}");
  });

  it("fields with types", () => {
    expect(
      <RustRoot>
        <StructDeclaration name="Foo">
          <List hardline>
            <StructField name="bar" type="i32" />
            <StructField name="baz" type="String" />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      struct Foo {
        bar: i32,
        baz: String,
      }
    `);
  });

  it("pub fields", () => {
    expect(
      <RustRoot>
        <StructDeclaration name="Foo">
          <List hardline>
            <StructField name="bar" type="i32" pub />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      struct Foo {
        pub bar: i32,
      }
    `);
  });

  it("pub struct", () => {
    expect(
      <RustRoot><StructDeclaration name="Foo" pub /></RustRoot>
    ).toRenderTo("pub struct Foo;");
  });

  it("derive attribute", () => {
    expect(
      <RustRoot>
        <StructDeclaration name="Foo" derive={["Debug", "Clone"]}>
          <List hardline>
            <StructField name="x" type="i32" />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      #[derive(Debug, Clone)]
      struct Foo {
        x: i32,
      }
    `);
  });

  it("generic type parameter", () => {
    expect(
      <RustRoot>
        <StructDeclaration name="Foo" typeParams={[{ name: "T" }]}>
          <List hardline>
            <StructField name="bar" type="T" />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      struct Foo<T> {
        bar: T,
      }
    `);
  });

  it("generic with trait bounds", () => {
    expect(
      <RustRoot>
        <StructDeclaration name="Foo" typeParams={[{ name: "T", bounds: ["Display", "Clone"] }]}>
          <List hardline>
            <StructField name="bar" type="T" />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      struct Foo<T: Display + Clone> {
        bar: T,
      }
    `);
  });

  it("lifetime parameter", () => {
    expect(
      <RustRoot>
        <StructDeclaration name="Foo" lifetimes={[{ name: "a" }]}>
          <List hardline>
            <StructField name="bar" type="&'a str" />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      struct Foo<'a> {
        bar: &'a str,
      }
    `);
  });

  it("mixed lifetime + generic", () => {
    expect(
      <RustRoot>
        <StructDeclaration
          name="Foo"
          lifetimes={[{ name: "a" }]}
          typeParams={[{ name: "T", bounds: ["'a"] }]}
        >
          <List hardline>
            <StructField name="bar" type="&'a T" />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      struct Foo<'a, T: 'a> {
        bar: &'a T,
      }
    `);
  });

  it("where clause", () => {
    expect(
      <RustRoot>
        <StructDeclaration
          name="Foo"
          typeParams={[{ name: "T" }]}
          where={[{ target: "T", bounds: ["Serialize"] }]}
        >
          <List hardline>
            <StructField name="bar" type="T" />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      struct Foo<T>
      where
          T: Serialize,
       {
        bar: T,
      }
    `);
  });
});

describe("TupleStructDeclaration", () => {
  it("basic tuple struct", () => {
    expect(
      <RustRoot><TupleStructDeclaration name="Point" fields={["f64", "f64"]} /></RustRoot>
    ).toRenderTo("struct Point(f64, f64);");
  });

  it("with derive", () => {
    expect(
      <RustRoot>
        <TupleStructDeclaration name="Point" derive={["Debug"]} fields={["f64", "f64"]} />
      </RustRoot>
    ).toRenderTo(`
      #[derive(Debug)]
      struct Point(f64, f64);
    `);
  });
});
