import { List, Output, Scope, createScope } from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { RustLexicalScope } from "../../scopes/01_lexical.js";
import { StructDeclaration, StructField } from "../1_declarations/0_StructDeclaration.js";
import { EnumDeclaration, UnitVariant } from "../1_declarations/1_EnumDeclaration.js";
import { serdeContainerAttr, serdeFieldAttr } from "./2_Serde.js";

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
  it("tagged enum", () => {
    expect(serdeContainerAttr({ tag: "type", content: "data" }))
      .toBe('serde(tag = "type", content = "data")');
  });

  it("untagged", () => {
    expect(serdeContainerAttr({ untagged: true }))
      .toBe("serde(untagged)");
  });

  it("multiple options", () => {
    expect(serdeContainerAttr({ denyUnknownFields: true, default: true }))
      .toBe("serde(deny_unknown_fields, default)");
  });

  it("empty config returns null", () => {
    expect(serdeContainerAttr({})).toBeNull();
  });

  it("transparent", () => {
    expect(serdeContainerAttr({ transparent: true }))
      .toBe("serde(transparent)");
  });
});

describe("serdeFieldAttr", () => {
  it("skip", () => {
    expect(serdeFieldAttr({ skip: true })).toBe("serde(skip)");
  });

  it("default (bool)", () => {
    expect(serdeFieldAttr({ default: true })).toBe("serde(default)");
  });

  it("default (path)", () => {
    expect(serdeFieldAttr({ default: "default_port" }))
      .toBe('serde(default = "default_port")');
  });

  it("flatten", () => {
    expect(serdeFieldAttr({ flatten: true })).toBe("serde(flatten)");
  });

  it("skip_serializing_if", () => {
    expect(serdeFieldAttr({ skipSerializingIf: "Option::is_none" }))
      .toBe('serde(skip_serializing_if = "Option::is_none")');
  });

  it("alias", () => {
    expect(serdeFieldAttr({ alias: "id" }))
      .toBe('serde(alias = "id")');
  });

  it("empty config returns null", () => {
    expect(serdeFieldAttr({})).toBeNull();
  });
});

describe("serde prop on StructDeclaration", () => {
  it("struct with deny_unknown_fields", () => {
    expect(
      <RustRoot>
        <StructDeclaration name="Config" derive={["Deserialize"]} serde={{ denyUnknownFields: true }}>
          <List hardline>
            <StructField name="port" type="u16" />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      #[derive(Deserialize)]
      #[serde(deny_unknown_fields)]
      struct Config {
        port: u16,
      }
    `);
  });

  it("struct with default", () => {
    expect(
      <RustRoot>
        <StructDeclaration name="Settings" derive={["Deserialize"]} serde={{ default: true }}>
          <List hardline>
            <StructField name="port" type="u16" />
            <StructField name="host" type="String" />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      #[derive(Deserialize)]
      #[serde(default)]
      struct Settings {
        port: u16,
        host: String,
      }
    `);
  });
});

describe("serde prop on StructField", () => {
  it("field with skip", () => {
    expect(
      <RustRoot>
        <StructDeclaration name="Foo">
          <List hardline>
            <StructField name="visible" type="i32" />
            <StructField name="hidden" type="String" serde={{ skip: true }} />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      struct Foo {
        visible: i32,
        #[serde(skip)]
        hidden: String,
      }
    `);
  });

  it("field with default", () => {
    expect(
      <RustRoot>
        <StructDeclaration name="Foo">
          <List hardline>
            <StructField name="id" type="u64" serde={{ default: true }} />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      struct Foo {
        #[serde(default)]
        id: u64,
      }
    `);
  });

  it("field with flatten", () => {
    expect(
      <RustRoot>
        <StructDeclaration name="Outer">
          <List hardline>
            <StructField name="inner" type="Inner" serde={{ flatten: true }} />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      struct Outer {
        #[serde(flatten)]
        inner: Inner,
      }
    `);
  });

  it("field with skip_serializing_if", () => {
    expect(
      <RustRoot>
        <StructDeclaration name="Foo">
          <List hardline>
            <StructField name="maybe" type="Option<String>" serde={{ skipSerializingIf: "Option::is_none" }} />
          </List>
        </StructDeclaration>
      </RustRoot>
    ).toRenderTo(`
      struct Foo {
        #[serde(skip_serializing_if = "Option::is_none")]
        maybe: Option<String>,
      }
    `);
  });
});

describe("serde prop on EnumDeclaration", () => {
  it("tagged enum", () => {
    expect(
      <RustRoot>
        <EnumDeclaration name="Event" derive={["Serialize", "Deserialize"]} serde={{ tag: "type", content: "data" }}>
          <List hardline>
            <UnitVariant name="Click" />
            <UnitVariant name="Scroll" />
          </List>
        </EnumDeclaration>
      </RustRoot>
    ).toRenderTo(`
      #[derive(Serialize, Deserialize)]
      #[serde(tag = "type", content = "data")]
      enum Event {
        Click,
        Scroll,
      }
    `);
  });

  it("untagged enum", () => {
    expect(
      <RustRoot>
        <EnumDeclaration name="Value" derive={["Deserialize"]} serde={{ untagged: true }}>
          <List hardline>
            <UnitVariant name="Str" />
            <UnitVariant name="Num" />
          </List>
        </EnumDeclaration>
      </RustRoot>
    ).toRenderTo(`
      #[derive(Deserialize)]
      #[serde(untagged)]
      enum Value {
        Str,
        Num,
      }
    `);
  });
});
