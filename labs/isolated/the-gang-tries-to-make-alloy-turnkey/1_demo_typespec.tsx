import { Children, Output, refkey, render } from "@alloy-js/core";
import {
  BuiltinType, EnumBody, EnumMember, EnumStatement, Identifier, IdentifierOrMemberExpression, InterfaceBody,
  InterfaceMember, InterfaceStatement, ModelBody, ModelExpression, ModelProperty, ModelStatement,
  OperationArguments, OperationSignatureDeclaration, PlainIdentifier, ReferenceExpression, ScalarExtends,
  ScalarStatement, SourceFile as TspSourceFile,
} from "./gen/typespec/0_nodes.js";
import { SourceFile } from "./typespec/3_SourceFile.js";

type Prop = { name: string; type: string };
export const model = {
  scalars: [{ name: "UnixMs", extends: "int64" }],
  enums: [{ name: "Status", members: ["Active", "Disabled"] }],
  models: [{
    name: "User",
    props: [{ name: "id", type: "int64" }, { name: "createdAt", type: "UnixMs" }, { name: "status", type: "Status" }] as Prop[],
  }],
  interfaces: [{ name: "UserStore", ops: [{ name: "get", params: [{ name: "id", type: "int64" }] as Prop[], returns: "User" }] }],
};

const BUILTINS = new Set(["int64", "int32", "string", "boolean"]);
const key = (name: string) => refkey("model", name);

function typeOf(t: string): Children {
  const id = BUILTINS.has(t) ? <BuiltinType>{t}</BuiltinType> : <PlainIdentifier>{key(t)}</PlainIdentifier>;
  return <ReferenceExpression><IdentifierOrMemberExpression><Identifier>{id}</Identifier></IdentifierOrMemberExpression></ReferenceExpression>;
}
// `sep` after every property; `,` between parameters only
const props = (ps: Prop[], sep: string, trailing: boolean) => ({
  keywords: (trailing ? ps : ps.slice(1)).map(() => sep),
  children: ps.map((p) => <ModelProperty name={p.name} type={typeOf(p.type)} />),
});

const spec = (
  <TspSourceFile>
    {[
      ...model.scalars.map((s) => (
        <ScalarStatement name={s.name} refkey={key(s.name)}>{<ScalarExtends>{typeOf(s.extends)}</ScalarExtends>}</ScalarStatement>
      )),
      ...model.enums.map((e) => (
        <EnumStatement name={e.name} refkey={key(e.name)}>
          <EnumBody keywords={e.members.map(() => ",")}>{e.members.map((m) => <EnumMember name={m} />)}</EnumBody>
        </EnumStatement>
      )),
      ...model.models.map((m) => (
        <ModelStatement name={m.name} refkey={key(m.name)}>
          <ModelExpression><ModelBody {...props(m.props, ";", true)} /></ModelExpression>
        </ModelStatement>
      )),
      ...model.interfaces.map((i) => (
        <InterfaceStatement name={i.name} refkey={key(i.name)}>
          <InterfaceBody>
            {i.ops.map((op) => (
              <InterfaceMember keywords={["op"]} name={op.name}>
                <OperationSignatureDeclaration>
                  {[<OperationArguments {...props(op.params, ",", false)} />, typeOf(op.returns)]}
                </OperationSignatureDeclaration>
              </InterfaceMember>
            ))}
          </InterfaceBody>
        </InterfaceStatement>
      )),
    ]}
  </TspSourceFile>
);

export function emitTsp(): string {
  const out = render(<Output><SourceFile path="api.tsp">{spec}</SourceFile></Output>);
  const file = out.contents[0];
  if (!("contents" in file) || typeof file.contents !== "string") throw new Error("no api.tsp");
  return file.contents.endsWith("\n") ? file.contents : file.contents + "\n";
}
