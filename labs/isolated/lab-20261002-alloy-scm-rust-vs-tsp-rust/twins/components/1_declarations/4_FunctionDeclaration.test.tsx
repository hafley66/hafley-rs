import { probe, probeTest } from "../../../4_probe.js";
import { Output, Scope, createScope } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { RustLexicalScope } from "../../../2_components.js";
import { FunctionDeclaration } from "../../../2_components.js";

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
  it("no params, no return", () => probeTest(["c117dbd32f6caac4"], () => {
    probe("c117dbd32f6caac4", () => (<RustRoot><FunctionDeclaration name="foo" /></RustRoot>), "fn foo() { }");
  }));

  it("params with types", () => probeTest(["16aeff7380c58c9d"], () => {
    probe("16aeff7380c58c9d", () => (<RustRoot>
        <FunctionDeclaration
          name="foo"
          params={[
            { name: "bar", type: "i32" },
            { name: "baz", type: "String" },
          ]}
        />
      </RustRoot>), "fn foo(bar: i32, baz: String) { }");
  }));

  it("return type", () => probeTest(["8542a69c294fd15f"], () => {
    probe("8542a69c294fd15f", () => (<RustRoot><FunctionDeclaration name="foo" returns="i32" /></RustRoot>), "fn foo() -> i32 { }");
  }));

  it("body content", () => probeTest(["8ce1ef6d3c6a8103"], () => {
    probe("8ce1ef6d3c6a8103", () => (<RustRoot>
        <FunctionDeclaration name="foo" returns="i32">
          42
        </FunctionDeclaration>
      </RustRoot>), `
      fn foo() -> i32 {
        42
      }
    `);
  }));

  it("generic", () => probeTest(["656a08a3c2a2fea0"], () => {
    probe("656a08a3c2a2fea0", () => (<RustRoot>
        <FunctionDeclaration
          name="foo"
          typeParams={[{ name: "T" }]}
          params={[{ name: "bar", type: "T" }]}
          returns="T"
        />
      </RustRoot>), "fn foo<T>(bar: T) -> T { }");
  }));

  it("trait bounds", () => probeTest(["432849ff6b3675d6"], () => {
    probe("432849ff6b3675d6", () => (<RustRoot>
        <FunctionDeclaration
          name="foo"
          typeParams={[{ name: "T", bounds: ["Display"] }]}
          params={[{ name: "bar", type: "T" }]}
        />
      </RustRoot>), "fn foo<T: Display>(bar: T) { }");
  }));

  it("where clause", () => probeTest(["55c8b17a90b727a2"], () => {
    probe("55c8b17a90b727a2", () => (<RustRoot>
        <FunctionDeclaration
          name="foo"
          typeParams={[{ name: "T" }]}
          params={[{ name: "bar", type: "T" }]}
          where={[{ target: "T", bounds: ["Serialize"] }]}
        />
      </RustRoot>), `
      fn foo<T>(bar: T)
      where
          T: Serialize,
       { }
    `);
  }));

  it("lifetime params", () => probeTest(["5f18efb60701dbae"], () => {
    probe("5f18efb60701dbae", () => (<RustRoot>
        <FunctionDeclaration
          name="foo"
          lifetimes={[{ name: "a" }]}
          params={[{ name: "bar", type: "&'a str" }]}
          returns="&'a str"
        />
      </RustRoot>), "fn foo<'a>(bar: &'a str) -> &'a str { }");
  }));

  it("pub visibility", () => probeTest(["050db53debf58f21"], () => {
    probe("050db53debf58f21", () => (<RustRoot><FunctionDeclaration name="foo" pub /></RustRoot>), "pub fn foo() { }");
  }));

  it("async", () => probeTest(["6c90ec995799bb24"], () => {
    probe("6c90ec995799bb24", () => (<RustRoot><FunctionDeclaration name="foo" async /></RustRoot>), "async fn foo() { }");
  }));

  it("pub async with everything", () => probeTest(["99554895de81abd1"], () => {
    probe("99554895de81abd1", () => (<RustRoot>
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
      </RustRoot>), `
      pub async fn fetch<T: DeserializeOwned>(url: &str) -> Result<T> {
        todo!()
      }
    `);
  }));
});
