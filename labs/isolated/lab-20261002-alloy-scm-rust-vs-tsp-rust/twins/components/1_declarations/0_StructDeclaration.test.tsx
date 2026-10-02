import { probe, probeTest } from "../../../4_probe.js";
import { List, Output, Scope, createScope } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { RustLexicalScope } from "../../../2_components.js";
import { StructDeclaration, StructField, TupleStructDeclaration } from "../../../2_components.js";

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
  it("unit struct (no body)", () => probeTest(["e8239b66e6665c0c"], () => {
    probe("e8239b66e6665c0c", () => (<RustRoot><StructDeclaration name="Foo" /></RustRoot>), "struct Foo;");
  }));

  it("empty braced struct", () => probeTest(["1c00d496c03c082b"], () => {
    probe("1c00d496c03c082b", () => (<RustRoot><StructDeclaration name="Foo" braced /></RustRoot>), "struct Foo {}");
  }));

  it("fields with types", () => probeTest(["e679f902837cf26f"], () => {
    probe("e679f902837cf26f", () => (<RustRoot>
        <StructDeclaration name="Foo">
          <List hardline>
            <StructField name="bar" type="i32" />
            <StructField name="baz" type="String" />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      struct Foo {
        bar: i32,
        baz: String,
      }
    `);
  }));

  it("pub fields", () => probeTest(["95829ce940db0807"], () => {
    probe("95829ce940db0807", () => (<RustRoot>
        <StructDeclaration name="Foo">
          <List hardline>
            <StructField name="bar" type="i32" pub />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      struct Foo {
        pub bar: i32,
      }
    `);
  }));

  it("pub struct", () => probeTest(["c0e643b49ab803df"], () => {
    probe("c0e643b49ab803df", () => (<RustRoot><StructDeclaration name="Foo" pub /></RustRoot>), "pub struct Foo;");
  }));

  it("derive attribute", () => probeTest(["5a72fa15d5d39cef"], () => {
    probe("5a72fa15d5d39cef", () => (<RustRoot>
        <StructDeclaration name="Foo" derive={["Debug", "Clone"]}>
          <List hardline>
            <StructField name="x" type="i32" />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      #[derive(Debug, Clone)]
      struct Foo {
        x: i32,
      }
    `);
  }));

  it("generic type parameter", () => probeTest(["c7dbc74971958f27"], () => {
    probe("c7dbc74971958f27", () => (<RustRoot>
        <StructDeclaration name="Foo" typeParams={[{ name: "T" }]}>
          <List hardline>
            <StructField name="bar" type="T" />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      struct Foo<T> {
        bar: T,
      }
    `);
  }));

  it("generic with trait bounds", () => probeTest(["4195a5e07d0a5715"], () => {
    probe("4195a5e07d0a5715", () => (<RustRoot>
        <StructDeclaration name="Foo" typeParams={[{ name: "T", bounds: ["Display", "Clone"] }]}>
          <List hardline>
            <StructField name="bar" type="T" />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      struct Foo<T: Display + Clone> {
        bar: T,
      }
    `);
  }));

  it("lifetime parameter", () => probeTest(["ea1b7fcf364690b9"], () => {
    probe("ea1b7fcf364690b9", () => (<RustRoot>
        <StructDeclaration name="Foo" lifetimes={[{ name: "a" }]}>
          <List hardline>
            <StructField name="bar" type="&'a str" />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      struct Foo<'a> {
        bar: &'a str,
      }
    `);
  }));

  it("mixed lifetime + generic", () => probeTest(["2459d838a186cecd"], () => {
    probe("2459d838a186cecd", () => (<RustRoot>
        <StructDeclaration
          name="Foo"
          lifetimes={[{ name: "a" }]}
          typeParams={[{ name: "T", bounds: ["'a"] }]}
        >
          <List hardline>
            <StructField name="bar" type="&'a T" />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      struct Foo<'a, T: 'a> {
        bar: &'a T,
      }
    `);
  }));

  it("where clause", () => probeTest(["956e24906361d245"], () => {
    probe("956e24906361d245", () => (<RustRoot>
        <StructDeclaration
          name="Foo"
          typeParams={[{ name: "T" }]}
          where={[{ target: "T", bounds: ["Serialize"] }]}
        >
          <List hardline>
            <StructField name="bar" type="T" />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      struct Foo<T>
      where
          T: Serialize,
       {
        bar: T,
      }
    `);
  }));
});

describe("TupleStructDeclaration", () => {
  it("basic tuple struct", () => probeTest(["dec4e68ff5f7acde"], () => {
    probe("dec4e68ff5f7acde", () => (<RustRoot><TupleStructDeclaration name="Point" fields={["f64", "f64"]} /></RustRoot>), "struct Point(f64, f64);");
  }));

  it("with derive", () => probeTest(["6f04d7539896e5ec"], () => {
    probe("6f04d7539896e5ec", () => (<RustRoot>
        <TupleStructDeclaration name="Point" derive={["Debug"]} fields={["f64", "f64"]} />
      </RustRoot>), `
      #[derive(Debug)]
      struct Point(f64, f64);
    `);
  }));
});
