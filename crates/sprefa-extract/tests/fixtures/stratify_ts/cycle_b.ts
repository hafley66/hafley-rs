import { cycleA } from "./cycle_a";

export function cycleB() {
  cycleA();
  privateThree();
  privateFour();
}

function privateThree() {}
function privateFour() {}
