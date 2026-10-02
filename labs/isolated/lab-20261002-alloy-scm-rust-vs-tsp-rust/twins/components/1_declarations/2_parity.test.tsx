import { probe, probeTest } from "../../../4_probe.js";
import { describe, expect, it } from "vitest";
import { List, Output, Scope, createScope } from "@alloy-js/core";
import { RustLexicalScope } from "../../../2_components.js";

import { LineComment, BlockComment, DocComment } from "../../../2_components.js";
import { Ref, BoxType, RcType, ArcType, OptionType, VecType, ResultType } from "../../../2_components.js";
import { TypeAlias } from "../../../2_components.js";
import { TraitDeclaration, TraitMethod, AssociatedType } from "../../../2_components.js";
import { ConstDeclaration, StaticDeclaration, LetDeclaration } from "../../../2_components.js";
import { FunctionDeclaration } from "../../../2_components.js";
import { StructDeclaration } from "../../../2_components.js";
import { ImplBlock } from "../../../2_components.js";

function RustRoot(props: { children: any }) {
  const scope = createScope(RustLexicalScope, "root", undefined);
  return <Output><Scope value={scope}>{props.children}</Scope></Output>;
}

describe("Comment components", () => {
  it("renders line comments", () => probeTest(["cab6fc5ee8913a7c"], () => {
    probe("cab6fc5ee8913a7c", () => (<RustRoot><LineComment>hello world</LineComment></RustRoot>), "// hello world");
  }));

  it("renders block comments", () => probeTest(["daf7762fcb5e2abc"], () => {
    probe("daf7762fcb5e2abc", () => (<RustRoot><BlockComment>multi line content</BlockComment></RustRoot>), "/* multi line content */");
  }));

  it("renders doc comments", () => probeTest(["497927f9e80b16cd"], () => {
    probe("497927f9e80b16cd", () => (<RustRoot><DocComment>Documentation for this item</DocComment></RustRoot>), "/// Documentation for this item");
  }));
});

describe("Reference type wrappers", () => {
  it("renders &T", () => probeTest(["abbed1592728e082"], () => {
    probe("abbed1592728e082", () => (<RustRoot><Ref>String</Ref></RustRoot>), "&String");
  }));

  it("renders &mut T", () => probeTest(["2bd7e0b285c0c202"], () => {
    probe("2bd7e0b285c0c202", () => (<RustRoot><Ref mut>Vec&lt;u8&gt;</Ref></RustRoot>), "&mut Vec<u8>");
  }));

  it("renders &'a T", () => probeTest(["86356afa94ee8e2c"], () => {
    probe("86356afa94ee8e2c", () => (<RustRoot><Ref lifetime="a">str</Ref></RustRoot>), "&'a str");
  }));

  it("renders &'a mut T", () => probeTest(["81063877ce5ef4c6"], () => {
    probe("81063877ce5ef4c6", () => (<RustRoot><Ref lifetime="a" mut>str</Ref></RustRoot>), "&'a mut str");
  }));

  it("renders Box<T>", () => probeTest(["e076867b4da18eca"], () => {
    probe("e076867b4da18eca", () => (<RustRoot><BoxType>dyn Error</BoxType></RustRoot>), "Box<dyn Error>");
  }));

  it("renders Rc<T>", () => probeTest(["e55dba5cca44cbe1"], () => {
    probe("e55dba5cca44cbe1", () => (<RustRoot><RcType>RefCell&lt;State&gt;</RcType></RustRoot>), "Rc<RefCell<State>>");
  }));

  it("renders Arc<T>", () => probeTest(["5a041e41aaacc0cc"], () => {
    probe("5a041e41aaacc0cc", () => (<RustRoot><ArcType>Mutex&lt;Data&gt;</ArcType></RustRoot>), "Arc<Mutex<Data>>");
  }));

  it("renders Option<T>", () => probeTest(["74874fc1c5082173"], () => {
    probe("74874fc1c5082173", () => (<RustRoot><OptionType>String</OptionType></RustRoot>), "Option<String>");
  }));

  it("renders Vec<T>", () => probeTest(["08292d00ac4f6054"], () => {
    probe("08292d00ac4f6054", () => (<RustRoot><VecType>u8</VecType></RustRoot>), "Vec<u8>");
  }));

  it("renders Result<T, E>", () => probeTest(["1da7186cecac8e77"], () => {
    probe("1da7186cecac8e77", () => (<RustRoot><ResultType ok="User" err="AppError" /></RustRoot>), "Result<User, AppError>");
  }));
});

describe("TypeAlias", () => {
  it("renders simple type alias", () => probeTest(["be0cea5dad78bf46"], () => {
    probe("be0cea5dad78bf46", () => (<RustRoot><TypeAlias name="UserId">i64</TypeAlias></RustRoot>), "type UserId = i64;");
  }));

  it("renders pub type alias", () => probeTest(["459d96ae618f926c"], () => {
    probe("459d96ae618f926c", () => (<RustRoot><TypeAlias name="Name" pub>String</TypeAlias></RustRoot>), "pub type Name = String;");
  }));

  it("renders generic type alias", () => probeTest(["09ba65b929ee1196"], () => {
    probe("09ba65b929ee1196", () => (<RustRoot>
      <TypeAlias name="Pair" typeParams={[{ name: "T" }]}>
        (T, T)
      </TypeAlias>
    </RustRoot>), "type Pair<T> = (T, T);");
  }));

  it("renders type alias with attributes", () => probeTest(["11027ab47f5ee23a"], () => {
    probe("11027ab47f5ee23a", () => (<RustRoot>
      <TypeAlias name="Handler" attrs={["allow(dead_code)"]}>
        Box&lt;dyn Fn()&gt;
      </TypeAlias>
    </RustRoot>), `
      #[allow(dead_code)]
      type Handler = Box<dyn Fn()>;
    `);
  }));
});

describe("TraitDeclaration", () => {
  it("renders empty trait", () => probeTest(["5823a2e5dc52781c"], () => {
    probe("5823a2e5dc52781c", () => (<RustRoot><TraitDeclaration name="Marker" /></RustRoot>), "trait Marker {}");
  }));

  it("renders pub trait", () => probeTest(["73d505b14b1192bb"], () => {
    probe("73d505b14b1192bb", () => (<RustRoot><TraitDeclaration name="Service" pub /></RustRoot>), "pub trait Service {}");
  }));

  it("renders trait with supertraits", () => probeTest(["c6674c8c7255b7e4"], () => {
    probe("c6674c8c7255b7e4", () => (<RustRoot>
      <TraitDeclaration name="Animal" supertraits={["Clone", "Debug"]} />
    </RustRoot>), "trait Animal: Clone + Debug {}");
  }));

  it("renders trait with methods", () => probeTest(["43d51813deddcb90"], () => {
    probe("43d51813deddcb90", () => (<RustRoot>
      <TraitDeclaration name="Greeter">
        <TraitMethod name="greet" selfParam="&" returns="String" />
      </TraitDeclaration>
    </RustRoot>), `
      trait Greeter {
        fn greet(&self) -> String;
      }
    `);
  }));

  it("renders trait with associated type", () => probeTest(["d32a38b303baeb36"], () => {
    probe("d32a38b303baeb36", () => (<RustRoot>
      <TraitDeclaration name="Iterator">
        <AssociatedType name="Item" />
        {"\n"}
        <TraitMethod name="next" selfParam="&mut" returns={<>Option&lt;Self::Item&gt;</>} />
      </TraitDeclaration>
    </RustRoot>), `
      trait Iterator {
        type Item;
        fn next(&mut self) -> Option<Self::Item>;
      }
    `);
  }));

  it("renders trait with associated type bounds", () => probeTest(["ae7043ba34c714c1"], () => {
    probe("ae7043ba34c714c1", () => (<RustRoot>
      <TraitDeclaration name="Container">
        <AssociatedType name="Item" bounds={["Clone", "Send"]} />
      </TraitDeclaration>
    </RustRoot>), `
      trait Container {
        type Item: Clone + Send;
      }
    `);
  }));

  it("renders generic trait with where clause", () => probeTest(["af72cd30fcd898d9"], () => {
    probe("af72cd30fcd898d9", () => (<RustRoot>
      <TraitDeclaration
        name="Store"
        typeParams={[{ name: "T" }]}
        where={[{ target: "T", bounds: ["Serialize", "Send"] }]}
      >
        <TraitMethod name="save" selfParam="&" params={[{ name: "item", type: <>&amp;T</> }]} />
      </TraitDeclaration>
    </RustRoot>), `
      trait Store<T>
      where
          T: Serialize + Send,
       {
        fn save(&self, item: &T);
      }
    `);
  }));

  it("renders trait with multiple methods", () => probeTest(["3cb7a67db418bc5c"], () => {
    probe("3cb7a67db418bc5c", () => (<RustRoot>
      <TraitDeclaration name="ReadWrite" pub>
        <List hardline>
          <TraitMethod name="read" selfParam="&" returns={<>Vec&lt;u8&gt;</>} />
          <TraitMethod name="write" selfParam="&mut" params={[{ name: "data", type: <>&amp;[u8]</> }]} />
        </List>
      </TraitDeclaration>
    </RustRoot>), `
      pub trait ReadWrite {
        fn read(&self) -> Vec<u8>;
        fn write(&mut self, data: &[u8]);
      }
    `);
  }));
});

describe("ConstDeclaration", () => {
  it("renders const", () => probeTest(["4a1a3c7e8fbddc62"], () => {
    probe("4a1a3c7e8fbddc62", () => (<RustRoot>
      <ConstDeclaration name="MAX_SIZE" type="usize">1024</ConstDeclaration>
    </RustRoot>), "const MAX_SIZE: usize = 1024;");
  }));

  it("renders pub const", () => probeTest(["3ea97a28fd590733"], () => {
    probe("3ea97a28fd590733", () => (<RustRoot>
      <ConstDeclaration name="VERSION" type={<>&amp;str</>} pub>"1.0.0"</ConstDeclaration>
    </RustRoot>), `pub const VERSION: &str = "1.0.0";`);
  }));

  it("renders const with attributes", () => probeTest(["295da65abbd7418e"], () => {
    probe("295da65abbd7418e", () => (<RustRoot>
      <ConstDeclaration name="PI" type="f64" attrs={["allow(clippy::approx_constant)"]}>
        3.14159
      </ConstDeclaration>
    </RustRoot>), `
      #[allow(clippy::approx_constant)]
      const PI: f64 = 3.14159;
    `);
  }));
});

describe("StaticDeclaration", () => {
  it("renders static", () => probeTest(["cf7a3bc09ade2815"], () => {
    probe("cf7a3bc09ade2815", () => (<RustRoot>
      <StaticDeclaration name="COUNTER" type="AtomicUsize">AtomicUsize::new(0)</StaticDeclaration>
    </RustRoot>), "static COUNTER: AtomicUsize = AtomicUsize::new(0);");
  }));

  it("renders static mut", () => probeTest(["e5cafb0ffe9bc8c9"], () => {
    probe("e5cafb0ffe9bc8c9", () => (<RustRoot>
      <StaticDeclaration name="BUFFER" type={<>[u8; 1024]</>} mut>[0u8; 1024]</StaticDeclaration>
    </RustRoot>), "static mut BUFFER: [u8; 1024] = [0u8; 1024];");
  }));
});

describe("LetDeclaration", () => {
  it("renders let with type and value", () => probeTest(["782501a372ca9065"], () => {
    probe("782501a372ca9065", () => (<RustRoot>
      <LetDeclaration name="x" type="i32">42</LetDeclaration>
    </RustRoot>), "let x: i32 = 42;");
  }));

  it("renders let mut", () => probeTest(["fe04f08caf565f86"], () => {
    probe("fe04f08caf565f86", () => (<RustRoot>
      <LetDeclaration name="buf" mut type={<>Vec&lt;u8&gt;</>}>Vec::new()</LetDeclaration>
    </RustRoot>), "let mut buf: Vec<u8> = Vec::new();");
  }));

  it("renders let without type annotation", () => probeTest(["2706fec5b975bef4"], () => {
    probe("2706fec5b975bef4", () => (<RustRoot>
      <LetDeclaration name="items">vec![1, 2, 3]</LetDeclaration>
    </RustRoot>), "let items = vec![1, 2, 3];");
  }));

  it("renders let without initializer", () => probeTest(["a955433e2aede4ce"], () => {
    probe("a955433e2aede4ce", () => (<RustRoot>
      <LetDeclaration name="result" type="Option&lt;String&gt;" />
    </RustRoot>), "let result: Option<String>;");
  }));
});

describe("trait + impl integration", () => {
  it("renders a trait and its impl block together", () => probeTest(["48ed451e9d842357"], () => {
    probe("48ed451e9d842357", () => (<RustRoot>
      <TraitDeclaration name="Greet" pub>
        <TraitMethod name="hello" selfParam="&" returns="String" />
      </TraitDeclaration>
      {"\n\n"}
      <StructDeclaration name="Bot" derive={["Debug"]} />
      {"\n\n"}
      <ImplBlock trait="Greet" target="Bot">
        <FunctionDeclaration name="hello" selfParam="&" returns="String">
          String::from("beep boop")
        </FunctionDeclaration>
      </ImplBlock>
    </RustRoot>), `
      pub trait Greet {
        fn hello(&self) -> String;
      }

      #[derive(Debug)]
      struct Bot;

      impl Greet for Bot {
        fn hello(&self) -> String {
          String::from("beep boop")
        }
      }
    `);
  }));
});
