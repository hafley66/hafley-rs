import { probe, probeTest } from "../../../4_probe.js";
import { List, Output, Scope, createScope } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { RustLexicalScope } from "../../../2_components.js";
import { StructDeclaration, StructField } from "../../../2_components.js";
import { EnumDeclaration, UnitVariant } from "../../../2_components.js";
import { serdeContainerAttr, serdeFieldAttr } from "../../../2_components.js";

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

describe("serdeContainerAttr", () => {
  it("tagged enum", () => probeTest(["d80832a451f433ad"], () => {
    probe("d80832a451f433ad", () => (serdeContainerAttr({ tag: "type", content: "data" })), 'serde(tag = "type", content = "data")');
  }));

  it("untagged", () => probeTest(["73a922a28caa8782"], () => {
    probe("73a922a28caa8782", () => (serdeContainerAttr({ untagged: true })), "serde(untagged)");
  }));

  it("multiple options", () => probeTest(["c637e98a1954fa97"], () => {
    probe("c637e98a1954fa97", () => (serdeContainerAttr({ denyUnknownFields: true, default: true })), "serde(deny_unknown_fields, default)");
  }));

  it("empty config returns null", () => probeTest(["5ff0355c62323f29"], () => {
    probe("5ff0355c62323f29", () => (serdeContainerAttr({})), null);
  }));

  it("transparent", () => probeTest(["cec1b25ae9dcf023"], () => {
    probe("cec1b25ae9dcf023", () => (serdeContainerAttr({ transparent: true })), "serde(transparent)");
  }));
});

describe("serdeFieldAttr", () => {
  it("skip", () => probeTest(["2dfa9506560cd31f"], () => {
    probe("2dfa9506560cd31f", () => (serdeFieldAttr({ skip: true })), "serde(skip)");
  }));

  it("default (bool)", () => probeTest(["5b0aa1b6d65a4781"], () => {
    probe("5b0aa1b6d65a4781", () => (serdeFieldAttr({ default: true })), "serde(default)");
  }));

  it("default (path)", () => probeTest(["39fbe7b23fcbe8c8"], () => {
    probe("39fbe7b23fcbe8c8", () => (serdeFieldAttr({ default: "default_port" })), 'serde(default = "default_port")');
  }));

  it("flatten", () => probeTest(["424a51e1b70cd60e"], () => {
    probe("424a51e1b70cd60e", () => (serdeFieldAttr({ flatten: true })), "serde(flatten)");
  }));

  it("skip_serializing_if", () => probeTest(["d46d66f2343d388a"], () => {
    probe("d46d66f2343d388a", () => (serdeFieldAttr({ skipSerializingIf: "Option::is_none" })), 'serde(skip_serializing_if = "Option::is_none")');
  }));

  it("alias", () => probeTest(["a3d80994df229f0f"], () => {
    probe("a3d80994df229f0f", () => (serdeFieldAttr({ alias: "id" })), 'serde(alias = "id")');
  }));

  it("empty config returns null", () => probeTest(["23235a5f413f5f6b"], () => {
    probe("23235a5f413f5f6b", () => (serdeFieldAttr({})), null);
  }));
});

describe("serde prop on StructDeclaration", () => {
  it("struct with deny_unknown_fields", () => probeTest(["8c058d3f48e3d7e7"], () => {
    probe("8c058d3f48e3d7e7", () => (<RustRoot>
        <StructDeclaration name="Config" derive={["Deserialize"]} serde={{ denyUnknownFields: true }}>
          <List hardline>
            <StructField name="port" type="u16" />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      #[derive(Deserialize)]
      #[serde(deny_unknown_fields)]
      struct Config {
        port: u16,
      }
    `);
  }));

  it("struct with default", () => probeTest(["371e384068d3af43"], () => {
    probe("371e384068d3af43", () => (<RustRoot>
        <StructDeclaration name="Settings" derive={["Deserialize"]} serde={{ default: true }}>
          <List hardline>
            <StructField name="port" type="u16" />
            <StructField name="host" type="String" />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      #[derive(Deserialize)]
      #[serde(default)]
      struct Settings {
        port: u16,
        host: String,
      }
    `);
  }));
});

describe("serde prop on StructField", () => {
  it("field with skip", () => probeTest(["88db30449168afb7"], () => {
    probe("88db30449168afb7", () => (<RustRoot>
        <StructDeclaration name="Foo">
          <List hardline>
            <StructField name="visible" type="i32" />
            <StructField name="hidden" type="String" serde={{ skip: true }} />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      struct Foo {
        visible: i32,
        #[serde(skip)]
        hidden: String,
      }
    `);
  }));

  it("field with default", () => probeTest(["b21a82a575282f63"], () => {
    probe("b21a82a575282f63", () => (<RustRoot>
        <StructDeclaration name="Foo">
          <List hardline>
            <StructField name="id" type="u64" serde={{ default: true }} />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      struct Foo {
        #[serde(default)]
        id: u64,
      }
    `);
  }));

  it("field with flatten", () => probeTest(["8646c02792e8c67b"], () => {
    probe("8646c02792e8c67b", () => (<RustRoot>
        <StructDeclaration name="Outer">
          <List hardline>
            <StructField name="inner" type="Inner" serde={{ flatten: true }} />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      struct Outer {
        #[serde(flatten)]
        inner: Inner,
      }
    `);
  }));

  it("field with skip_serializing_if", () => probeTest(["4dbb01c798f20bac"], () => {
    probe("4dbb01c798f20bac", () => (<RustRoot>
        <StructDeclaration name="Foo">
          <List hardline>
            <StructField name="maybe" type="Option<String>" serde={{ skipSerializingIf: "Option::is_none" }} />
          </List>
        </StructDeclaration>
      </RustRoot>), `
      struct Foo {
        #[serde(skip_serializing_if = "Option::is_none")]
        maybe: Option<String>,
      }
    `);
  }));
});

describe("serde prop on EnumDeclaration", () => {
  it("tagged enum", () => probeTest(["644f7c41b0c89129"], () => {
    probe("644f7c41b0c89129", () => (<RustRoot>
        <EnumDeclaration name="Event" derive={["Serialize", "Deserialize"]} serde={{ tag: "type", content: "data" }}>
          <List hardline>
            <UnitVariant name="Click" />
            <UnitVariant name="Scroll" />
          </List>
        </EnumDeclaration>
      </RustRoot>), `
      #[derive(Serialize, Deserialize)]
      #[serde(tag = "type", content = "data")]
      enum Event {
        Click,
        Scroll,
      }
    `);
  }));

  it("untagged enum", () => probeTest(["811a37f6fc8e2c06"], () => {
    probe("811a37f6fc8e2c06", () => (<RustRoot>
        <EnumDeclaration name="Value" derive={["Deserialize"]} serde={{ untagged: true }}>
          <List hardline>
            <UnitVariant name="Str" />
            <UnitVariant name="Num" />
          </List>
        </EnumDeclaration>
      </RustRoot>), `
      #[derive(Deserialize)]
      #[serde(untagged)]
      enum Value {
        Str,
        Num,
      }
    `);
  }));
});
