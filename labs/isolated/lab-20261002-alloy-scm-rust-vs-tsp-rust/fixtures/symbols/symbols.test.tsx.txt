import {
  Output,
  Declaration,
  Name,
  Scope,
  createScope,
  refkey,
  render,
} from "@alloy-js/core";
import { describe, expect, it } from "vitest";
import { RustSymbol } from "./00_rust.js";
import { RustNamedTypeSymbol } from "./01_named-type.js";
import { RustFunctionSymbol } from "./02_function.js";
import { RustLexicalScope } from "../scopes/01_lexical.js";
import { RustNamedTypeScope } from "../scopes/04_named-type.js";
import { createTypeSymbol, createFieldSymbol, createFunctionSymbol } from "./03_factories.js";

// Minimal wrapper that provides a RustLexicalScope for symbol creation
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

describe("symbol creation", () => {
  it("creates a struct symbol via factory", () => {
    const key = refkey();
    function Inner() {
      const sym = createTypeSymbol("MyStruct", "struct", { refkeys: key });
      return (
        <Declaration symbol={sym}>
          struct <Name /> {"{}"}
        </Declaration>
      );
    }

    expect(
      <RustRoot>
        <Inner />
      </RustRoot>
    ).toRenderTo("struct MyStruct {}");
  });

  it("creates a function symbol via factory", () => {
    function Inner() {
      const sym = createFunctionSymbol("do_thing");
      return (
        <Declaration symbol={sym}>
          fn <Name />() {"{ }"}
        </Declaration>
      );
    }

    expect(
      <RustRoot>
        <Inner />
      </RustRoot>
    ).toRenderTo("fn do_thing() { }");
  });

  it("creates field symbols inside a named type scope", () => {
    function Inner() {
      const structSym = createTypeSymbol("Foo", "struct");
      const typeScope = createScope(RustNamedTypeScope, structSym, undefined);

      function Fields() {
        const fieldSym = createFieldSymbol("bar");
        return (
          <Declaration symbol={fieldSym}>
            <Name />: i32,
          </Declaration>
        );
      }

      return (
        <Declaration symbol={structSym}>
          struct <Name /> {"{"}
          <Scope value={typeScope}>
            {"\n  "}<Fields />
          </Scope>
          {"\n}"}
        </Declaration>
      );
    }

    expect(
      <RustRoot>
        <Inner />
      </RustRoot>
    ).toRenderTo(`
      struct Foo {
        bar: i32,
      }
    `);
  });

  it("RustNamedTypeSymbol tracks typeKind reactively", () => {
    function Inner() {
      const sym = createTypeSymbol("Thing", "type");
      expect(sym.typeKind).toBe("type");
      sym.typeKind = "struct";
      expect(sym.typeKind).toBe("struct");
      return <><Name /></>;
    }

    expect(
      <RustRoot>
        <Declaration symbol={createTypeSymbol("X", "type")}>
          <Inner />
        </Declaration>
      </RustRoot>
    ).toRenderTo("X");
  });
});
