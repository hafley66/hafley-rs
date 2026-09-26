export type Boxed<T> = { value: T };

export function identity<T>(value: T): T {
  return value;
}

export const echo = <T>(value: T): T => value;

export function makeHolder<T>(value: T) {
  return class<U> {
    outer!: T;
    inner!: U;
  };
}
