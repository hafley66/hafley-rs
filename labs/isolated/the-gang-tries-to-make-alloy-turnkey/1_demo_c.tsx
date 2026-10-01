import { Children, Output, refkey, render } from "@alloy-js/core";
import { SourceFile } from "./c/3_SourceFile.js";
import {
  Enumerator, EnumeratorList, EnumSpecifier, FieldDeclaration, FieldDeclarationList, FieldIdentifier,
  FunctionDeclarator, Identifier, ParameterDeclaration, ParameterList, ParenthesizedDeclarator,
  PointerDeclarator, PreprocInclude, PrimitiveType, SizedTypeSpecifier, StructSpecifier,
  SystemLibString, TranslationUnit, TypeDefinition, TypeIdentifier,
} from "./gen/c/0_nodes.js";

type Ty = { type: string; pointer?: boolean };
type Member = Ty & { name: string };
export const model = {
  aliases: [{ name: "UnixMs", type: "int64_t" }],
  enums: [{ name: "Status", members: ["Status_Active", "Status_Disabled"] }],
  structs: [{
    name: "User",
    fields: [
      { name: "id", type: "int64_t" },
      { name: "created_at", type: "UnixMs" },
      { name: "status", type: "Status" },
      { name: "display_name", type: "char", pointer: true },
    ] as Member[],
  }],
  interfaces: [{
    name: "UserStore",
    ops: [
      { name: "get", returns: "int32_t", params: [{ name: "id", type: "int64_t" }, { name: "out", type: "User", pointer: true }] as Member[] },
      { name: "list", returns: "int32_t", params: [{ name: "out", type: "User", pointer: true }, { name: "cap", type: "unsigned long" }] as Member[] },
    ],
  }],
};

const PRIMITIVES = new Set(["int64_t", "int32_t", "char", "void"]);
const key = (name: string) => refkey("model", name);

function typeOf(t: string): Children {
  if (PRIMITIVES.has(t)) return <PrimitiveType>{t}</PrimitiveType>;
  if (t.startsWith("unsigned ")) return <SizedTypeSpecifier keywords={t.split(" ")} />;
  return <TypeIdentifier>{key(t)}</TypeIdentifier>;
}
const ptr = (on: boolean | undefined, d: Children) => (on ? <PointerDeclarator declarator={d} /> : d);

const header = (
  <TranslationUnit>
    {[
      <PreprocInclude path={<SystemLibString>{"<stdint.h>"}</SystemLibString>} />,
      ...model.aliases.map((a) => <TypeDefinition type={typeOf(a.type)} declarator={a.name} refkey={key(a.name)} />),
      ...model.enums.map((e) => (
        <TypeDefinition
          type={<EnumSpecifier name={e.name} body={<EnumeratorList>{e.members.map((m) => <Enumerator name={m} />)}</EnumeratorList>} />}
          declarator={e.name}
          refkey={key(e.name)}
        />
      )),
      ...model.structs.map((s) => (
        <TypeDefinition
          type={<StructSpecifier name={s.name} body={
            <FieldDeclarationList>
              {s.fields.map((f) => <FieldDeclaration type={typeOf(f.type)} declarator={f.pointer ? ptr(true, <FieldIdentifier>{f.name}</FieldIdentifier>) : f.name} />)}
            </FieldDeclarationList>
          } />}
          declarator={s.name}
          refkey={key(s.name)}
        />
      )),
      ...model.interfaces.map((i) => (
        <TypeDefinition
          type={<StructSpecifier name={i.name} body={
            <FieldDeclarationList>
              {i.ops.map((op) => (
                <FieldDeclaration
                  type={typeOf(op.returns)}
                  declarator={
                    <FunctionDeclarator
                      declarator={<ParenthesizedDeclarator><PointerDeclarator declarator={<FieldIdentifier>{op.name}</FieldIdentifier>} /></ParenthesizedDeclarator>}
                      parameters={
                        <ParameterList>
                          {[{ name: "self", type: "void", pointer: true }, ...op.params].map((p) => (
                            <ParameterDeclaration type={typeOf(p.type)} declarator={ptr(p.pointer, <Identifier>{p.name}</Identifier>)} />
                          ))}
                        </ParameterList>
                      }
                    />
                  }
                />
              ))}
            </FieldDeclarationList>
          } />}
          declarator={i.name}
          refkey={key(i.name)}
        />
      )),
    ]}
  </TranslationUnit>
);

export function emitHeader(): string {
  const out = render(<Output><SourceFile path="api.h">{header}</SourceFile></Output>);
  const file = out.contents[0];
  if (!("contents" in file) || typeof file.contents !== "string") throw new Error("no api.h");
  return file.contents.endsWith("\n") ? file.contents : file.contents + "\n";
}
