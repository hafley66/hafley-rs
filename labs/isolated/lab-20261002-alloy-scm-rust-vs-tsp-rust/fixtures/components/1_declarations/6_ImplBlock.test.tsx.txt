import { List, Output, Scope, createScope } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { RustLexicalScope } from "../../scopes/01_lexical.js";
import { FunctionDeclaration } from "./4_FunctionDeclaration.js";
import { ImplBlock } from "./6_ImplBlock.js";

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

describe("ImplBlock", () => {
  it("inherent impl, empty", () => {
    expect(
      <RustRoot><ImplBlock target="Foo" /></RustRoot>
    ).toRenderTo("impl Foo { }");
  });

  it("inherent impl with method", () => {
    expect(
      <RustRoot>
        <ImplBlock target="Foo">
          <FunctionDeclaration name="bar" pub>
            42
          </FunctionDeclaration>
        </ImplBlock>
      </RustRoot>
    ).toRenderTo(`
      impl Foo {
        pub fn bar() {
          42
        }
      }
    `);
  });

  it("trait impl", () => {
    expect(
      <RustRoot>
        <ImplBlock target="Foo" trait="Display">
          <FunctionDeclaration
            name="fmt"
            selfParam="&"
            params={[
              { name: "f", type: "&mut fmt::Formatter<'_>" },
            ]}
            returns="fmt::Result"
          >
            write!(f, "Foo")
          </FunctionDeclaration>
        </ImplBlock>
      </RustRoot>
    ).toRenderTo(`
      impl Display for Foo {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
          write!(f, "Foo")
        }
      }
    `);
  });

  it("generic impl", () => {
    expect(
      <RustRoot><ImplBlock target="Foo<T>" typeParams={[{ name: "T" }]} /></RustRoot>
    ).toRenderTo("impl<T> Foo<T> { }");
  });

  it("impl with where clause", () => {
    expect(
      <RustRoot>
        <ImplBlock
          target="Foo<T>"
          typeParams={[{ name: "T" }]}
          where={[{ target: "T", bounds: ["Clone"] }]}
        />
      </RustRoot>
    ).toRenderTo(`
      impl<T> Foo<T>
      where
          T: Clone,
       { }
    `);
  });

  it("multiple impl blocks for same type", () => {
    expect(
      <RustRoot>
        <ImplBlock target="Foo">
          <FunctionDeclaration name="new" returns="Self">
            todo!()
          </FunctionDeclaration>
        </ImplBlock>
        {"\n\n"}
        <ImplBlock target="Foo" trait="Default">
          <FunctionDeclaration name="default" returns="Self">
            todo!()
          </FunctionDeclaration>
        </ImplBlock>
      </RustRoot>
    ).toRenderTo(`
      impl Foo {
        fn new() -> Self {
          todo!()
        }
      }

      impl Default for Foo {
        fn default() -> Self {
          todo!()
        }
      }
    `);
  });

  it("trait impl with generics on both", () => {
    expect(
      <RustRoot>
        <ImplBlock
          target="Vec<T>"
          trait="From<T>"
          typeParams={[{ name: "T" }]}
        >
          <FunctionDeclaration
            name="from"
            params={[{ name: "item", type: "T" }]}
            returns="Self"
          >
            vec![item]
          </FunctionDeclaration>
        </ImplBlock>
      </RustRoot>
    ).toRenderTo(`
      impl<T> From<T> for Vec<T> {
        fn from(item: T) -> Self {
          vec![item]
        }
      }
    `);
  });
});
