import { isLit, makeNode, Policy } from "./0_print.js";
export { Leaf } from "./0_print.js";

// Fallback for languages with no hand layer: one space between tokens, no symbols.
export const PLAIN: Policy = {
  space: (_kind, prev, t) => !isLit(prev, "(") && ![";", ",", ")"].some((s) => isLit(t, s)),
  lines: new Set(), block: new Set(), list: new Set(),
};
export const Node = makeNode(PLAIN);
