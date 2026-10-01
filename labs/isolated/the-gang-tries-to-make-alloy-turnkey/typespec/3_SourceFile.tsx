import { Children, computed, emitSymbol, NamePolicyContext, Refkey, resolve, SourceFile as CoreSourceFile } from "@alloy-js/core";
import { createTspNamePolicy } from "./0_name-policy.js";
import { TspScope, TspSymbol } from "./1_scope.js";

// Single-file TypeSpec: a resolved refkey prints the declared name; no imports or namespaces.
export function Reference(props: { refkey: Refkey }) {
  const result = resolve<TspScope, TspSymbol>(props.refkey);
  const name = computed(() => result.value?.lexicalDeclaration.name ?? "<Unresolved Symbol>");
  if (result.value) emitSymbol(result.value.symbol);
  return <>{name.value}</>;
}

export function SourceFile(props: { path: string; children?: Children }) {
  return (
    <NamePolicyContext.Provider value={createTspNamePolicy()}>
      <CoreSourceFile path={props.path} filetype="typespec" reference={Reference}>
        {props.children}
      </CoreSourceFile>
    </NamePolicyContext.Provider>
  );
}
