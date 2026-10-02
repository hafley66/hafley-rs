import { createScope, createSymbol, Declaration, Name, type Refkey, Scope, useScope } from "@alloy-js/core";
import { isLit, makeNode, type Policy } from "../core/0_print.js";
import { useRustNamePolicy } from "./0_name-policy.js";
import { RustScope, RustSymbol, SPACE_OF } from "./1_scope.js";
export { Leaf } from "../core/0_print.js";
const TIGHT = new Set(["attribute_item", "token_tree", "type_arguments", "type_parameters", "lifetime", "generic_type", "scoped_type_identifier", "scoped_identifier"]);
export const RUST_POLICY: Policy = {
  space(kind, prev, tok) {
    if (TIGHT.has(kind)) return isLit(prev, ",");
    if (kind === "reference_type") return !isLit(prev, "&");
    if (["parameters", "arguments", "ordered_field_declaration_list", "tuple_type"].includes(kind)) return isLit(prev, ",") || isLit(prev, ";");
    if ([";", ",", ")", "]", ">", ":", "::"].some(s => isLit(tok, s))) return false;
    if (["(", "[", "<", "::"].some(s => isLit(prev, s))) return false;
    if (["parameters", "type_parameters"].some(s => "val" in tok && tok.src === s)) return false;
    if (kind === "trait_bounds") return !isLit(prev, ":");
    return true;
  },
  lines: new Set(["source_file"]),
  block: new Set(["field_declaration_list", "enum_variant_list", "declaration_list", "block"]),
  list: new Set(),
  declare(kind, name, refkey) {
    const scope = useScope() as RustScope;
    const symbol = createSymbol(RustSymbol, name, scope.spaceFor(SPACE_OF[kind] ?? "values")!, {
      refkeys: refkey as Refkey | undefined, namePolicy: useRustNamePolicy().for(kind), binder: scope.binder,
    });
    return <Declaration symbol={symbol}><Name /></Declaration>;
  },
  scope: (kind, body) => <Scope value={createScope(RustScope, kind, useScope() as RustScope | undefined)}>{body}</Scope>,
};
export const Node = makeNode(RUST_POLICY);
