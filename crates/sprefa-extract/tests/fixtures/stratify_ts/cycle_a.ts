import { cycleB } from "./cycle_b";

export function cycleA() {
  cycleB();
  privateOne();
  privateTwo();
}

function privateOne() {}
function privateTwo() {}
