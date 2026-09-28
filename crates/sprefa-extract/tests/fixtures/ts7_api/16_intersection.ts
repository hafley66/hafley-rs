type A = { old: number };
type B = { old: number };
declare const value: A & B;
console.log(value.old);
export {};
