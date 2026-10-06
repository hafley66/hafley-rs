export class Animal {
  sound(): string {
    return "";
  }
}
export function feed(fn: (a: Animal) => string): string {
  return fn(new Animal());
}
