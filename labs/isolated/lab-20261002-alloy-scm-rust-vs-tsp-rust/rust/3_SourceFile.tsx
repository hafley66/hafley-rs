import {
  type Children, computed, createScope, emitSymbol, For, NamePolicyContext, type Refkey,
  resolve, Scope, Show, SourceDirectory, SourceFile as CoreSourceFile, useScope,
} from "@alloy-js/core";
import { createRustNamePolicy } from "./0_name-policy.js";
import { RustModuleScope, RustScope, RustSourceFileScope, RustSymbol, sourceFileOf } from "./1_scope.js";
export function Reference(props: { refkey: Refkey }) {
  const file = sourceFileOf(useScope());
  const result = resolve<RustScope, RustSymbol>(props.refkey);
  const name = computed(() => {
    if (!result.value) return "<Unresolved Symbol>";
    const { lexicalDeclaration, memberPath } = result.value;
    const target = sourceFileOf(lexicalDeclaration.scope);
    if (file && target && file !== target && target.declarations.has(lexicalDeclaration)) {
      file.uses.set(["crate", ...target.modulePath, lexicalDeclaration.name].join("::"), lexicalDeclaration);
    }
    return [lexicalDeclaration.name, ...memberPath.map(symbol => symbol.name)].join("::");
  });
  if (result.value) emitSymbol(result.value.symbol);
  return <>{name.value}</>;
}
export function CrateDirectory(props: { path?: string; children?: Children }) {
  const scope = createScope(RustModuleScope, "crate", undefined);
  return <NamePolicyContext.Provider value={createRustNamePolicy()}>
    <SourceDirectory path={props.path ?? "."}><Scope value={scope}>{props.children}</Scope></SourceDirectory>
  </NamePolicyContext.Provider>;
}
export function ModDirectory(props: { name: string; children?: Children }) {
  const parent = useScope();
  const scope = createScope(RustModuleScope, props.name, parent);
  if (parent instanceof RustModuleScope) {
    parent.mods.add(props.name);
    scope.modulePath = [...parent.modulePath, props.name];
  }
  return <SourceDirectory path={props.name}><Scope value={scope}>
    <SourceFile path="mod.rs" />{props.children}
  </Scope></SourceDirectory>;
}
export function SourceFile(props: { path: string; externalUses?: string[]; children?: Children }) {
  const parent = useScope();
  const module = parent instanceof RustModuleScope ? parent : undefined;
  const root = ["lib.rs", "main.rs", "mod.rs"].includes(props.path);
  const scope = createScope(RustSourceFileScope, props.path, parent);
  scope.modulePath = [...(module?.modulePath ?? []), ...(root ? [] : [props.path.replace(/\.rs$/, "")])];
  if (!root) module?.mods.add(props.path.replace(/\.rs$/, ""));
  return <NamePolicyContext.Provider value={createRustNamePolicy()}>
    <CoreSourceFile path={props.path} filetype="rust" reference={Reference}><Scope value={scope}>
      <Show when={root && module && module.mods.size > 0}>
        <For each={[...(module?.mods ?? [])].sort()} joiner={<hbr />}>
          {(name: string) => <>pub mod {name};</>}
        </For><hbr /><hbr />
      </Show>
      <Show when={scope.uses.size > 0 || (props.externalUses?.length ?? 0) > 0}>
        <For each={[...new Set([...(props.externalUses ?? []), ...scope.uses.keys()])].sort()} joiner={<hbr />}>
          {(path: string) => <>use {path};</>}
        </For><hbr /><hbr />
      </Show>
      {props.children}
    </Scope></CoreSourceFile>
  </NamePolicyContext.Provider>;
}
