import { Box } from "./box";

export function read(box: Box): number {
  const { old } = box;
  return box.old + old;
}
