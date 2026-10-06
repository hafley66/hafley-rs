import { Animal, feed } from "./b";
export function pet(a: Animal): string {
  return a.sound();
}
export const run = feed(pet);
