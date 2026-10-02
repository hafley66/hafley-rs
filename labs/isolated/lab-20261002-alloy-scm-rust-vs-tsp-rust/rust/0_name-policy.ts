import { createNamePolicy, type NamePolicy, useNamePolicy } from "@alloy-js/core";
export type RustElements = string;
const KEYWORDS = new Set([
  "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
  "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
  "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true",
  "type", "unsafe", "use", "where", "while", "abstract", "become", "box", "do", "final", "macro",
  "override", "priv", "try", "typeof", "unsized", "virtual", "yield",
]);
export function createRustNamePolicy(): NamePolicy<RustElements> {
  return createNamePolicy((name, element) => element === "lifetime" || !KEYWORDS.has(name) ? name : `r#${name}`);
}
export function useRustNamePolicy(): NamePolicy<RustElements> { return useNamePolicy(); }
