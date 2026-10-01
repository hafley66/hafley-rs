import { createNamePolicy, NamePolicy, useNamePolicy } from "@alloy-js/core";

// element = the tree-sitter node kind that declares the name
export type TspElements =
  | "scalar_statement" | "enum_statement" | "model_statement" | "interface_statement"
  | "enum_member" | "model_property" | "interface_member";

// TypeSpec reserved words (compiler 1.x scanner keywords)
const TSP_KEYWORDS = new Set([
  "import", "model", "scalar", "namespace", "using", "op", "enum", "alias", "is", "interface", "union",
  "projection", "else", "if", "dec", "fn", "const", "init", "extern", "extends", "true", "false",
  "return", "void", "never", "unknown", "valueof", "typeof",
]);

// names kept as written; keyword collisions are backtick-quoted
export function createTspNamePolicy(): NamePolicy<TspElements> {
  return createNamePolicy((name) => (TSP_KEYWORDS.has(name) ? `\`${name}\`` : name));
}

export function useTspNamePolicy(): NamePolicy<TspElements> {
  return useNamePolicy();
}
