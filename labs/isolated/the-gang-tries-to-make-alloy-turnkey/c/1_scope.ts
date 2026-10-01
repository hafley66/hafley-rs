import { createSymbol, OutputScope, OutputSymbol } from "@alloy-js/core";
import type { CElements } from "./0_name-policy.js";

// C has a tag namespace (struct/enum/union names) and an ordinary namespace.
export class CScope extends OutputScope {
  public static readonly declarationSpaces = ["ordinary", "tags"];
}

export class CSymbol extends OutputSymbol {
  copy(): OutputSymbol {
    const copy = createSymbol(CSymbol, this.name, undefined, { ...this.getCopyOptions(), binder: this.binder });
    this.initializeCopy(copy);
    return copy;
  }
}

export const SPACE_OF: Record<CElements, "ordinary" | "tags"> = {
  type_definition: "ordinary",
  struct_specifier: "tags",
  enum_specifier: "tags",
  enumerator: "ordinary",
  field_declaration: "ordinary",
};
