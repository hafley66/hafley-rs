import type { Base } from "./_0_types";
import { Service } from "./_0_types";

export function one(input: Base): Base {
  const service = new Service();
  return service.ping(input);
}
