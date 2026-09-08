export const rowKey = Symbol.for('falcon-boundary.row');

export function $row(context, target, kind) {
  context.program.stateMap(rowKey).set(target, kind.asNumber());
}

export const $decorators = { FalconBoundary: { row: $row } };
