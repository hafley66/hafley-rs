import { Children, computed, emitSymbol, NamePolicyContext, Refkey, resolve, SourceFile as CoreSourceFile } from "@alloy-js/core";
import { createCNamePolicy } from "./0_name-policy.js";
import { CScope, CSymbol } from "./1_scope.js";

// Single-file C: a resolved refkey prints the declared name; includes stay hand-written.
export function Reference(props: { refkey: Refkey }) {
  const result = resolve<CScope, CSymbol>(props.refkey);
  const name = computed(() => result.value?.lexicalDeclaration.name ?? "<Unresolved Symbol>");
  if (result.value) emitSymbol(result.value.symbol);
  return <>{name.value}</>;
}

export function SourceFile(props: { path: string; children?: Children }) {
  return (
    <NamePolicyContext.Provider value={createCNamePolicy()}>
      <CoreSourceFile path={props.path} filetype="c" reference={Reference}>
        {props.children}
      </CoreSourceFile>
    </NamePolicyContext.Provider>
  );
}
