import { type Children, computed, emitSymbol, NamePolicyContext, type Refkey, resolve, SourceFile as CoreSourceFile } from "@alloy-js/core";
import { createRustNamePolicy } from "./0_name-policy.js";
import { RustScope, RustSymbol } from "./1_scope.js";
export function Reference(props: { refkey: Refkey }) {
  const result = resolve<RustScope, RustSymbol>(props.refkey);
  const name = computed(() => result.value?.lexicalDeclaration.name ?? "<Unresolved Symbol>");
  if (result.value) emitSymbol(result.value.symbol);
  return <>{name.value}</>;
}
export function SourceFile(props: { path: string; children?: Children }) {
  return <NamePolicyContext.Provider value={createRustNamePolicy()}>
    <CoreSourceFile path={props.path} filetype="rust" reference={Reference}>{props.children}</CoreSourceFile>
  </NamePolicyContext.Provider>;
}
