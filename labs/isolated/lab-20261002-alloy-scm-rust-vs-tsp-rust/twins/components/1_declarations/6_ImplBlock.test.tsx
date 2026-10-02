import { probe, probeTest } from "../../../4_probe.js";
import { List, Output, Scope, createScope } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { RustLexicalScope } from "../../../2_components.js";
import { FunctionDeclaration } from "../../../2_components.js";
import { ImplBlock } from "../../../2_components.js";

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
  it("inherent impl, empty", () => probeTest(["67c3cad083c1e2ba"], () => {
    probe("67c3cad083c1e2ba", () => (<RustRoot><ImplBlock target="Foo" /></RustRoot>), "impl Foo { }");
  }));

  it("inherent impl with method", () => probeTest(["e5f9a939718893c5"], () => {
    probe("e5f9a939718893c5", () => (<RustRoot>
        <ImplBlock target="Foo">
          <FunctionDeclaration name="bar" pub>
            42
          </FunctionDeclaration>
        </ImplBlock>
      </RustRoot>), `
      impl Foo {
        pub fn bar() {
          42
        }
      }
    `);
  }));

  it("trait impl", () => probeTest(["297b66bf43e3f366"], () => {
    probe("297b66bf43e3f366", () => (<RustRoot>
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
      </RustRoot>), `
      impl Display for Foo {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
          write!(f, "Foo")
        }
      }
    `);
  }));

  it("generic impl", () => probeTest(["a9b60b9f0e189101"], () => {
    probe("a9b60b9f0e189101", () => (<RustRoot><ImplBlock target="Foo<T>" typeParams={[{ name: "T" }]} /></RustRoot>), "impl<T> Foo<T> { }");
  }));

  it("impl with where clause", () => probeTest(["5db4d5b937eccde3"], () => {
    probe("5db4d5b937eccde3", () => (<RustRoot>
        <ImplBlock
          target="Foo<T>"
          typeParams={[{ name: "T" }]}
          where={[{ target: "T", bounds: ["Clone"] }]}
        />
      </RustRoot>), `
      impl<T> Foo<T>
      where
          T: Clone,
       { }
    `);
  }));

  it("multiple impl blocks for same type", () => probeTest(["e19bdf1c03009c8f"], () => {
    probe("e19bdf1c03009c8f", () => (<RustRoot>
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
      </RustRoot>), `
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
  }));

  it("trait impl with generics on both", () => probeTest(["b2507f6055aa6683"], () => {
    probe("b2507f6055aa6683", () => (<RustRoot>
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
      </RustRoot>), `
      impl<T> From<T> for Vec<T> {
        fn from(item: T) -> Self {
          vec![item]
        }
      }
    `);
  }));
});
