import { createSymbol, OutputScope, OutputSymbol } from "@alloy-js/core";

// TypeSpec: one declaration space per scope (namespace, model, enum, interface, op parameters).
export class TspScope extends OutputScope {
  public static readonly declarationSpaces = ["members"];
}

export class TspSymbol extends OutputSymbol {
  copy(): OutputSymbol {
    const copy = createSymbol(TspSymbol, this.name, undefined, { ...this.getCopyOptions(), binder: this.binder });
    this.initializeCopy(copy);
    return copy;
  }
}
