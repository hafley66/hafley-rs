// Twin-only intent adapters. Generated nodes print item/type syntax; opaque caller text and
// field/variant trailing commas are supplied here and included in the hand-written line count.
// Attributes are siblings of items in the grammar. High-level emitter APIs remain GAPs.
import { type Children, createScope, NamePolicyContext, Output, printTree, renderTree, Scope } from "@alloy-js/core";
import * as G from "./gen/rust/0_nodes.js";
import { createRustNamePolicy } from "./rust/0_name-policy.js";
import { RustScope } from "./rust/1_scope.js";
import { Gap } from "./1_gap.js";
export { SourceFile } from "./rust/3_SourceFile.js";
export { RustScope as RustLexicalScope } from "./rust/1_scope.js";
export { createRustNamePolicy, type RustElements } from "./rust/0_name-policy.js";
type Props = Record<string, any>;
function visibility(p: Props): Children { return p.pub ? <G.VisibilityModifier /> : undefined; }
function generics(p: Props): Children {
  const xs = [
    ...(p.lifetimes ?? []).map((x: Props) => <G.LifetimeParameter name={<G.Lifetime>{x.name}</G.Lifetime>} />),
    ...(p.typeParams ?? []).map((x: Props) => <G.TypeParameter name={x.name} bounds={x.bounds?.length ? <G.TraitBounds>{x.bounds}</G.TraitBounds> : undefined} />),
  ];
  return xs.length ? <G.TypeParameters>{xs}</G.TypeParameters> : undefined;
}
function whereClause(p: Props): Children {
  return p.where?.length ? <G.WhereClause>{p.where.map((x: Props) => <G.WherePredicate left={x.target} bounds={<G.TraitBounds>{x.bounds}</G.TraitBounds>} />)}</G.WhereClause> : undefined;
}
function attrs(p: Props): Children {
  const xs = [...(p.attrs ?? [])];
  if (p.derive?.length) xs.unshift(`derive(${p.derive.join(", ")})`);
  if (p.serde) {
    const s = serdeText(p.serde);
    if (s) xs.push(s);
  }
  return xs.map((x: string) => <><G.AttributeItem><G.Attribute>{x}</G.Attribute></G.AttributeItem><hbr /></>);
}
export function CrateDirectory(p: Props) {
  return <NamePolicyContext.Provider value={createRustNamePolicy()}><Scope value={createScope(RustScope, "crate", undefined)}>{p.children}</Scope></NamePolicyContext.Provider>;
}
export function ModDirectory(_p: Props): Children { throw new Gap("refkey/import resolution", "No module registration or nested directory synthesis in the grammar policy"); }
export function StructDeclaration(p: Props) {
  return <>{attrs(p)}<G.StructItem name={p.name} refkey={p.refkey} type_parameters={generics(p)} where_clause={whereClause(p)} body={p.braced || p.children ? <G.FieldDeclarationList>{p.children}</G.FieldDeclarationList> : undefined}>{visibility(p)}</G.StructItem></>;
}
export function StructField(p: Props) {
  return <>{attrs(p)}<G.FieldDeclaration name={p.name} type={p.type}>{visibility(p)}</G.FieldDeclaration>,</>;
}
export function TupleStructDeclaration(p: Props) {
  return <>{attrs(p)}<G.StructItem name={p.name} refkey={p.refkey} type_parameters={generics(p)} where_clause={whereClause(p)} body={<G.OrderedFieldDeclarationList type={p.fields} />}>{visibility(p)}</G.StructItem></>;
}
export function EnumDeclaration(p: Props) {
  return <>{attrs(p)}<G.EnumItem name={p.name} refkey={p.refkey} type_parameters={generics(p)} where_clause={whereClause(p)} body={<G.EnumVariantList>{p.children}</G.EnumVariantList>}>{visibility(p)}</G.EnumItem></>;
}
export function UnitVariant(p: Props) { return <><G.EnumVariant name={p.name} value={p.value} />,</>; }
export function TupleVariant(p: Props) { return <><G.EnumVariant name={p.name} body={<G.OrderedFieldDeclarationList type={p.fields} />} />,</>; }
export function StructVariant(p: Props) { return <><G.EnumVariant name={p.name} body={<G.FieldDeclarationList>{p.children}</G.FieldDeclarationList>} />,</>; }
function parameters(p: Props): Children {
  const xs: Children[] = [];
  if (p.selfParam) xs.push(p.selfParam === "&" ? "&self" : p.selfParam === "&mut" ? "&mut self" : "self");
  for (const x of p.params ?? []) xs.push(<G.Parameter pattern={x.name} type={x.type} />);
  return <G.Parameters>{xs}</G.Parameters>;
}
export function FunctionDeclaration(p: Props) {
  // function_modifiers has no consuming keyword prop. A caller-provided async modifier is an opaque child.
  return <>{attrs(p)}<G.FunctionItem name={p.name} refkey={p.refkey} parameters={parameters(p)} return_type={p.returns} type_parameters={generics(p)} where_clause={whereClause(p)} body={<G.Block>{p.children}</G.Block>}>{[visibility(p), p.async ? "async" : undefined]}</G.FunctionItem></>;
}
export function ImplBlock(p: Props) {
  return <G.ImplItem type={p.target} trait={p.trait} type_parameters={generics(p)} where_clause={whereClause(p)} body={<G.DeclarationList>{p.children}</G.DeclarationList>} />;
}
export function TypeAlias(p: Props) { return <>{attrs(p)}<G.TypeItem name={p.name} type_parameters={generics(p)} type={p.children}>{visibility(p)}</G.TypeItem></>; }
export function TraitDeclaration(p: Props) {
  return <>{attrs(p)}<G.TraitItem name={p.name} type_parameters={generics(p)} where_clause={whereClause(p)} bounds={p.supertraits?.length ? <G.TraitBounds>{p.supertraits}</G.TraitBounds> : undefined} body={<G.DeclarationList>{p.children}</G.DeclarationList>}>{visibility(p)}</G.TraitItem></>;
}
export function TraitMethod(p: Props) { return <G.FunctionSignatureItem name={p.name} parameters={parameters(p)} return_type={p.returns} type_parameters={generics(p)} where_clause={whereClause(p)} />; }
export function AssociatedType(p: Props) { return <G.AssociatedType name={p.name} bounds={p.bounds?.length ? <G.TraitBounds>{p.bounds}</G.TraitBounds> : undefined} />; }
export function ConstDeclaration(p: Props) { return <>{attrs(p)}<G.ConstItem name={p.name} type={p.type} value={p.children}>{visibility(p)}</G.ConstItem></>; }
export function StaticDeclaration(p: Props) { return <>{attrs(p)}<G.StaticItem mutable_specifier={p.mut ? <G.MutableSpecifier>mut</G.MutableSpecifier> : undefined} name={p.name} type={p.type} value={p.children}>{visibility(p)}</G.StaticItem></>; }
export function LetDeclaration(p: Props) { return <G.LetDeclaration pattern={p.name} type={p.type} value={p.children} mutable_specifier={p.mut ? <G.MutableSpecifier>mut</G.MutableSpecifier> : undefined} />; }
export function LineComment(_p: Props): Children { throw new Gap("doc comments", "Generated line_comment only exposes doc-marker fields; ordinary comment body is a pruned PATTERN"); }
export function BlockComment(_p: Props): Children { throw new Gap("doc comments", "Generated block_comment has no ordinary body prop"); }
export function DocComment(p: Props) { return <G.LineComment outer={<G.OuterDocCommentMarker>/</G.OuterDocCommentMarker>} doc={<G.DocComment>{p.children}</G.DocComment>} />; }
export function Ref(p: Props) { return <G.ReferenceType type={p.children} mutable_specifier={p.mut ? <G.MutableSpecifier>mut</G.MutableSpecifier> : undefined}>{p.lifetime ? <G.Lifetime>{p.lifetime}</G.Lifetime> : undefined}</G.ReferenceType>; }
export function BoxType(p: Props) { return <G.GenericType type="Box" type_arguments={<G.TypeArguments>{p.children}</G.TypeArguments>} />; }
export function RcType(p: Props) { return <G.GenericType type="Rc" type_arguments={<G.TypeArguments>{p.children}</G.TypeArguments>} />; }
export function ArcType(p: Props) { return <G.GenericType type="Arc" type_arguments={<G.TypeArguments>{p.children}</G.TypeArguments>} />; }
export function OptionType(p: Props) { return <G.GenericType type="Option" type_arguments={<G.TypeArguments>{p.children}</G.TypeArguments>} />; }
export function VecType(p: Props) { return <G.GenericType type="Vec" type_arguments={<G.TypeArguments>{p.children}</G.TypeArguments>} />; }
export function ResultType(p: Props) { return <G.GenericType type="Result" type_arguments={<G.TypeArguments>{[p.ok, p.err]}</G.TypeArguments>} />; }
const SERDE_NAMES: Record<string, string> = { denyUnknownFields: "deny_unknown_fields", skipSerializingIf: "skip_serializing_if", renameAll: "rename_all" };
function serdeText(p: Props): string | null {
  const xs = Object.entries(p).filter(([, v]) => v !== undefined && v !== false).map(([k,v]) => {
    const name = SERDE_NAMES[k] ?? k;
    return v === true ? name : `${name} = ${JSON.stringify(v)}`;
  });
  return xs.length ? `serde(${xs.join(", ")})` : null;
}
export function serdeContainerAttr(p: Props): string | null {
  const text = serdeText(p);
  return text === null ? null : printTree(renderTree(<Output><G.Attribute>{text}</G.Attribute></Output>));
}
export function serdeFieldAttr(p: Props): string | null { return serdeContainerAttr(p); }
