type A = { kind: "a"; old: number };
type B = { kind: "b"; old: number };
const value: A | B = { kind: "a", old: 1 };
console.log(value.old);
export {};
