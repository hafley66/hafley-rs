function bind_args(values: readonly number[]): number[] {
  return values.slice();
}

export function arrival_statement(row: readonly number[]): number[] {
  return bind_args(row);
}
