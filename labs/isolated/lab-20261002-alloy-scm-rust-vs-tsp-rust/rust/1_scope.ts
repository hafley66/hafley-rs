import { createSymbol, OutputScope, OutputSymbol, shallowReactive } from "@alloy-js/core";
export class RustScope extends OutputScope {
  static readonly declarationSpaces = ["types", "values", "members"];
}
export class RustModuleScope extends RustScope {
  modulePath: string[] = [];
  mods = shallowReactive(new Set<string>());
}
export class RustSourceFileScope extends RustScope {
  modulePath: string[] = [];
  declarations = shallowReactive(new Set<RustSymbol>());
  uses = shallowReactive(new Map<string, RustSymbol>());
}
export function sourceFileOf(scope: OutputScope | undefined): RustSourceFileScope | undefined {
  while (scope) {
    if (scope instanceof RustSourceFileScope) return scope;
    scope = scope.parent;
  }
  return undefined;
}
export class RustSymbol extends OutputSymbol {
  copy(): OutputSymbol {
    const copy = createSymbol(RustSymbol, this.name, undefined, { ...this.getCopyOptions(), binder: this.binder });
    this.initializeCopy(copy);
    return copy;
  }
}
export const SPACE_OF: Record<string, string> = {
  struct_item: "types", enum_item: "types", trait_item: "types", type_item: "types",
  function_item: "values", function_signature_item: "values", const_item: "values", static_item: "values",
  enum_variant: "members", field_declaration: "members", type_parameter: "types", associated_type: "types",
};
