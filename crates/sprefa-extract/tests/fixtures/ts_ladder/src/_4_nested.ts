import type { Base, Box } from "@core/_0_types";
import { Service } from "@core/_0_types";
import * as manyModule from "./_3_many";
import type { PublicBase } from "./_3_many";
import { one } from "./_3_many";

export class Nested extends Service {
  run<T extends Base>(input: Box<T>): Base {
    const child = new manyModule.Child();
    child.ping(input.value);
    return manyModule.many(input);
  }
}

export function reexported(input: PublicBase): PublicBase {
  return one(input);
}
