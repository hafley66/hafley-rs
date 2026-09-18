import { Defs } from "./defs";

export function mk(): void {}
export function push(_: number): void {}

export function f(d: Defs): void {
  d.push(1);
  const w: number[] = [];
  w.push(2);
  const q = mk();
  q.push(3);
  mk().push(4);
  push(5);
}
