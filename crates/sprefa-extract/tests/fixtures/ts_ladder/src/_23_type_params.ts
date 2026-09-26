export type Boxed<T> = { value: T };

export function identity<T>(value: T): T {
  return value;
}
