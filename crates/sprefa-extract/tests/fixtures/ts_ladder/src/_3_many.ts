import type { Base, Box } from "./_0_types";
import { Service, makeService } from "./_0_types";
import * as core from "./_0_types";

export { Base as PublicBase } from "./_0_types";
export * from "./_2_one";

export class Child extends Service {
  label = "child";
}

export function many<T extends Base>(input: Box<T>): Base {
  const service = makeService();
  service.ping(input.value);
  core.makeService().ping(input.value);
  return input.value;
}
