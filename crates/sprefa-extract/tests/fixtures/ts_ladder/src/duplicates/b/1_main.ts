import { shared } from "./0_shared.ts";

export function useShared(): number {
  return shared();
}
