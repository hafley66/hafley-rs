import { describe, expect, it } from "vitest";
import { List, Output, Scope, createScope } from "@alloy-js/core";
import { RustLexicalScope } from "../../scopes/01_lexical.js";

import { LineComment, BlockComment, DocComment } from "../0_primitives/3_Comment.js";
import { Ref, BoxType, RcType, ArcType, OptionType, VecType, ResultType } from "../0_primitives/4_ReferenceTypes.js";
import { TypeAlias } from "./3_TypeAlias.js";
import { TraitDeclaration, TraitMethod, AssociatedType } from "./2_TraitDeclaration.js";
import { ConstDeclaration, StaticDeclaration, LetDeclaration } from "./5_VariableDeclaration.js";
import { FunctionDeclaration } from "./4_FunctionDeclaration.js";
import { StructDeclaration } from "./0_StructDeclaration.js";
import { ImplBlock } from "./6_ImplBlock.js";

function RustRoot(props: { children: any }) {
  const scope = createScope(RustLexicalScope, "root", undefined);
  return <Output><Scope value={scope}>{props.children}</Scope></Output>;
}

describe("Comment components", () => {
  it("renders line comments", () => {
    expect(<RustRoot><LineComment>hello world</LineComment></RustRoot>)
      .toRenderTo("// hello world");
  });

  it("renders block comments", () => {
    expect(<RustRoot><BlockComment>multi line content</BlockComment></RustRoot>)
      .toRenderTo("/* multi line content */");
  });

  it("renders doc comments", () => {
    expect(<RustRoot><DocComment>Documentation for this item</DocComment></RustRoot>)
      .toRenderTo("/// Documentation for this item");
  });
});

describe("Reference type wrappers", () => {
  it("renders &T", () => {
    expect(<RustRoot><Ref>String</Ref></RustRoot>)
      .toRenderTo("&String");
  });

  it("renders &mut T", () => {
    expect(<RustRoot><Ref mut>Vec&lt;u8&gt;</Ref></RustRoot>)
      .toRenderTo("&mut Vec<u8>");
  });

  it("renders &'a T", () => {
    expect(<RustRoot><Ref lifetime="a">str</Ref></RustRoot>)
      .toRenderTo("&'a str");
  });

  it("renders &'a mut T", () => {
    expect(<RustRoot><Ref lifetime="a" mut>str</Ref></RustRoot>)
      .toRenderTo("&'a mut str");
  });

  it("renders Box<T>", () => {
    expect(<RustRoot><BoxType>dyn Error</BoxType></RustRoot>)
      .toRenderTo("Box<dyn Error>");
  });

  it("renders Rc<T>", () => {
    expect(<RustRoot><RcType>RefCell&lt;State&gt;</RcType></RustRoot>)
      .toRenderTo("Rc<RefCell<State>>");
  });

  it("renders Arc<T>", () => {
    expect(<RustRoot><ArcType>Mutex&lt;Data&gt;</ArcType></RustRoot>)
      .toRenderTo("Arc<Mutex<Data>>");
  });

  it("renders Option<T>", () => {
    expect(<RustRoot><OptionType>String</OptionType></RustRoot>)
      .toRenderTo("Option<String>");
  });

  it("renders Vec<T>", () => {
    expect(<RustRoot><VecType>u8</VecType></RustRoot>)
      .toRenderTo("Vec<u8>");
  });

  it("renders Result<T, E>", () => {
    expect(<RustRoot><ResultType ok="User" err="AppError" /></RustRoot>)
      .toRenderTo("Result<User, AppError>");
  });
});

describe("TypeAlias", () => {
  it("renders simple type alias", () => {
    expect(<RustRoot><TypeAlias name="UserId">i64</TypeAlias></RustRoot>)
      .toRenderTo("type UserId = i64;");
  });

  it("renders pub type alias", () => {
    expect(<RustRoot><TypeAlias name="Name" pub>String</TypeAlias></RustRoot>)
      .toRenderTo("pub type Name = String;");
  });

  it("renders generic type alias", () => {
    expect(<RustRoot>
      <TypeAlias name="Pair" typeParams={[{ name: "T" }]}>
        (T, T)
      </TypeAlias>
    </RustRoot>).toRenderTo("type Pair<T> = (T, T);");
  });

  it("renders type alias with attributes", () => {
    expect(<RustRoot>
      <TypeAlias name="Handler" attrs={["allow(dead_code)"]}>
        Box&lt;dyn Fn()&gt;
      </TypeAlias>
    </RustRoot>).toRenderTo(`
      #[allow(dead_code)]
      type Handler = Box<dyn Fn()>;
    `);
  });
});

describe("TraitDeclaration", () => {
  it("renders empty trait", () => {
    expect(<RustRoot><TraitDeclaration name="Marker" /></RustRoot>)
      .toRenderTo("trait Marker {}");
  });

  it("renders pub trait", () => {
    expect(<RustRoot><TraitDeclaration name="Service" pub /></RustRoot>)
      .toRenderTo("pub trait Service {}");
  });

  it("renders trait with supertraits", () => {
    expect(<RustRoot>
      <TraitDeclaration name="Animal" supertraits={["Clone", "Debug"]} />
    </RustRoot>).toRenderTo("trait Animal: Clone + Debug {}");
  });

  it("renders trait with methods", () => {
    expect(<RustRoot>
      <TraitDeclaration name="Greeter">
        <TraitMethod name="greet" selfParam="&" returns="String" />
      </TraitDeclaration>
    </RustRoot>).toRenderTo(`
      trait Greeter {
        fn greet(&self) -> String;
      }
    `);
  });

  it("renders trait with associated type", () => {
    expect(<RustRoot>
      <TraitDeclaration name="Iterator">
        <AssociatedType name="Item" />
        {"\n"}
        <TraitMethod name="next" selfParam="&mut" returns={<>Option&lt;Self::Item&gt;</>} />
      </TraitDeclaration>
    </RustRoot>).toRenderTo(`
      trait Iterator {
        type Item;
        fn next(&mut self) -> Option<Self::Item>;
      }
    `);
  });

  it("renders trait with associated type bounds", () => {
    expect(<RustRoot>
      <TraitDeclaration name="Container">
        <AssociatedType name="Item" bounds={["Clone", "Send"]} />
      </TraitDeclaration>
    </RustRoot>).toRenderTo(`
      trait Container {
        type Item: Clone + Send;
      }
    `);
  });

  it("renders generic trait with where clause", () => {
    expect(<RustRoot>
      <TraitDeclaration
        name="Store"
        typeParams={[{ name: "T" }]}
        where={[{ target: "T", bounds: ["Serialize", "Send"] }]}
      >
        <TraitMethod name="save" selfParam="&" params={[{ name: "item", type: <>&amp;T</> }]} />
      </TraitDeclaration>
    </RustRoot>).toRenderTo(`
      trait Store<T>
      where
          T: Serialize + Send,
       {
        fn save(&self, item: &T);
      }
    `);
  });

  it("renders trait with multiple methods", () => {
    expect(<RustRoot>
      <TraitDeclaration name="ReadWrite" pub>
        <List hardline>
          <TraitMethod name="read" selfParam="&" returns={<>Vec&lt;u8&gt;</>} />
          <TraitMethod name="write" selfParam="&mut" params={[{ name: "data", type: <>&amp;[u8]</> }]} />
        </List>
      </TraitDeclaration>
    </RustRoot>).toRenderTo(`
      pub trait ReadWrite {
        fn read(&self) -> Vec<u8>;
        fn write(&mut self, data: &[u8]);
      }
    `);
  });
});

describe("ConstDeclaration", () => {
  it("renders const", () => {
    expect(<RustRoot>
      <ConstDeclaration name="MAX_SIZE" type="usize">1024</ConstDeclaration>
    </RustRoot>).toRenderTo("const MAX_SIZE: usize = 1024;");
  });

  it("renders pub const", () => {
    expect(<RustRoot>
      <ConstDeclaration name="VERSION" type={<>&amp;str</>} pub>"1.0.0"</ConstDeclaration>
    </RustRoot>).toRenderTo(`pub const VERSION: &str = "1.0.0";`);
  });

  it("renders const with attributes", () => {
    expect(<RustRoot>
      <ConstDeclaration name="PI" type="f64" attrs={["allow(clippy::approx_constant)"]}>
        3.14159
      </ConstDeclaration>
    </RustRoot>).toRenderTo(`
      #[allow(clippy::approx_constant)]
      const PI: f64 = 3.14159;
    `);
  });
});

describe("StaticDeclaration", () => {
  it("renders static", () => {
    expect(<RustRoot>
      <StaticDeclaration name="COUNTER" type="AtomicUsize">AtomicUsize::new(0)</StaticDeclaration>
    </RustRoot>).toRenderTo("static COUNTER: AtomicUsize = AtomicUsize::new(0);");
  });

  it("renders static mut", () => {
    expect(<RustRoot>
      <StaticDeclaration name="BUFFER" type={<>[u8; 1024]</>} mut>[0u8; 1024]</StaticDeclaration>
    </RustRoot>).toRenderTo("static mut BUFFER: [u8; 1024] = [0u8; 1024];");
  });
});

describe("LetDeclaration", () => {
  it("renders let with type and value", () => {
    expect(<RustRoot>
      <LetDeclaration name="x" type="i32">42</LetDeclaration>
    </RustRoot>).toRenderTo("let x: i32 = 42;");
  });

  it("renders let mut", () => {
    expect(<RustRoot>
      <LetDeclaration name="buf" mut type={<>Vec&lt;u8&gt;</>}>Vec::new()</LetDeclaration>
    </RustRoot>).toRenderTo("let mut buf: Vec<u8> = Vec::new();");
  });

  it("renders let without type annotation", () => {
    expect(<RustRoot>
      <LetDeclaration name="items">vec![1, 2, 3]</LetDeclaration>
    </RustRoot>).toRenderTo("let items = vec![1, 2, 3];");
  });

  it("renders let without initializer", () => {
    expect(<RustRoot>
      <LetDeclaration name="result" type="Option&lt;String&gt;" />
    </RustRoot>).toRenderTo("let result: Option<String>;");
  });
});

describe("trait + impl integration", () => {
  it("renders a trait and its impl block together", () => {
    expect(<RustRoot>
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
    </RustRoot>).toRenderTo(`
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
  });
});
