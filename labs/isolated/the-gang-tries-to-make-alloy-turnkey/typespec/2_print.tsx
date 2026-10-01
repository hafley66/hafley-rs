import { createScope, createSymbol, Declaration, Name, Refkey, Scope, useScope } from "@alloy-js/core";
import { isLit, makeNode, Policy } from "../core/0_print.js";
import { TspElements, useTspNamePolicy } from "./0_name-policy.js";
import { TspScope, TspSymbol } from "./1_scope.js";
export { Leaf } from "../core/0_print.js";

const NO_SPACE_BEFORE = [";", ",", ")", ":", "?"];

export const TSP_POLICY: Policy = {
  space: (kind, prev, t) =>
    !isLit(prev, "(") && !NO_SPACE_BEFORE.some((s) => isLit(t, s)) &&
    !(kind === "interface_member" && "src" in prev && prev.src === "name"),
  lines: new Set(["source_file"]),
  block: new Set(["enum_body", "model_expression", "interface_body"]),
  list: new Set(["model_body"]),
  declare(kind, name, refkey) {
    const scope = useScope() as TspScope;
    const sym = createSymbol(TspSymbol, name, scope.spaceFor("members")!, {
      refkeys: refkey as Refkey | undefined,
      namePolicy: useTspNamePolicy().for(kind as TspElements),
      binder: scope.binder,
    });
    return <Declaration symbol={sym}><Name /></Declaration>;
  },
  scope: (kind, body) => <Scope value={createScope(TspScope, kind, useScope() as TspScope | undefined)}>{body}</Scope>,
};
export const Node = makeNode(TSP_POLICY);
