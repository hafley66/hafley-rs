import { Output, Scope, createScope } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { RustLexicalScope } from "../../scopes/01_lexical.js";
import { FunctionDeclaration } from "./4_FunctionDeclaration.js";

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

describe("FunctionDeclaration", () => {
  it("no params, no return", () => {
    expect(
      <RustRoot><FunctionDeclaration name="foo" /></RustRoot>
    ).toRenderTo("fn foo() { }");
  });

  it("params with types", () => {
    expect(
      <RustRoot>
        <FunctionDeclaration
          name="foo"
          params={[
            { name: "bar", type: "i32" },
            { name: "baz", type: "String" },
          ]}
        />
      </RustRoot>
    ).toRenderTo("fn foo(bar: i32, baz: String) { }");
  });

  it("return type", () => {
    expect(
      <RustRoot><FunctionDeclaration name="foo" returns="i32" /></RustRoot>
    ).toRenderTo("fn foo() -> i32 { }");
  });

  it("body content", () => {
    expect(
      <RustRoot>
        <FunctionDeclaration name="foo" returns="i32">
          42
        </FunctionDeclaration>
      </RustRoot>
    ).toRenderTo(`
      fn foo() -> i32 {
        42
      }
    `);
  });

  it("generic", () => {
    expect(
      <RustRoot>
        <FunctionDeclaration
          name="foo"
          typeParams={[{ name: "T" }]}
          params={[{ name: "bar", type: "T" }]}
          returns="T"
        />
      </RustRoot>
    ).toRenderTo("fn foo<T>(bar: T) -> T { }");
  });

  it("trait bounds", () => {
    expect(
      <RustRoot>
        <FunctionDeclaration
          name="foo"
          typeParams={[{ name: "T", bounds: ["Display"] }]}
          params={[{ name: "bar", type: "T" }]}
        />
      </RustRoot>
    ).toRenderTo("fn foo<T: Display>(bar: T) { }");
  });

  it("where clause", () => {
    expect(
      <RustRoot>
        <FunctionDeclaration
          name="foo"
          typeParams={[{ name: "T" }]}
          params={[{ name: "bar", type: "T" }]}
          where={[{ target: "T", bounds: ["Serialize"] }]}
        />
      </RustRoot>
    ).toRenderTo(`
      fn foo<T>(bar: T)
      where
          T: Serialize,
       { }
    `);
  });

  it("lifetime params", () => {
    expect(
      <RustRoot>
        <FunctionDeclaration
          name="foo"
          lifetimes={[{ name: "a" }]}
          params={[{ name: "bar", type: "&'a str" }]}
          returns="&'a str"
        />
      </RustRoot>
    ).toRenderTo("fn foo<'a>(bar: &'a str) -> &'a str { }");
  });

  it("pub visibility", () => {
    expect(
      <RustRoot><FunctionDeclaration name="foo" pub /></RustRoot>
    ).toRenderTo("pub fn foo() { }");
  });

  it("async", () => {
    expect(
      <RustRoot><FunctionDeclaration name="foo" async /></RustRoot>
    ).toRenderTo("async fn foo() { }");
  });

  it("pub async with everything", () => {
    expect(
      <RustRoot>
        <FunctionDeclaration
          name="fetch"
          pub
          async
          typeParams={[{ name: "T", bounds: ["DeserializeOwned"] }]}
          params={[{ name: "url", type: "&str" }]}
          returns="Result<T>"
        >
          todo!()
        </FunctionDeclaration>
      </RustRoot>
    ).toRenderTo(`
      pub async fn fetch<T: DeserializeOwned>(url: &str) -> Result<T> {
        todo!()
      }
    `);
  });
});
