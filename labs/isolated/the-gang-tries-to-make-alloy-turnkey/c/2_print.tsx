import { createScope, createSymbol, Declaration, Name, Refkey, Scope, useScope } from "@alloy-js/core";
import { isLit, makeNode, Policy } from "../core/0_print.js";
import { CElements, useCNamePolicy } from "./0_name-policy.js";
import { CScope, CSymbol, SPACE_OF } from "./1_scope.js";
export { Leaf } from "../core/0_print.js";

// Whitespace policy (hand-written, per node kind).
const TIGHT = new Set(["pointer_declarator", "parenthesized_declarator", "function_declarator", "parameter_list"]);

export const C_POLICY: Policy = {
  space: (kind, prev, t) =>
    TIGHT.has(kind) ? isLit(prev, ",") : !isLit(prev, "(") && ![";", ",", ")"].some((s) => isLit(t, s)),
  block: new Set(["field_declaration_list", "enumerator_list"]),
  lines: new Set(["translation_unit"]),
  list: new Set(),
  declare(kind, name, refkey) {
    const scope = useScope() as CScope;
    const sym = createSymbol(CSymbol, name, scope.spaceFor(SPACE_OF[kind as CElements])!, {
      refkeys: refkey as Refkey | undefined,
      namePolicy: useCNamePolicy().for(kind as CElements),
      binder: scope.binder,
    });
    return <Declaration symbol={sym}><Name /></Declaration>;
  },
  scope: (kind, body) => <Scope value={createScope(CScope, kind, useScope() as CScope | undefined)}>{body}</Scope>,
};
export const Node = makeNode(C_POLICY);
