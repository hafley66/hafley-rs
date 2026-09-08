export const rowKey = Symbol.for('falcon-boundary.row');
export const godotKey = Symbol.for('falcon-boundary.godot');
export const packedKey = Symbol.for('falcon-boundary.packed');

export function $row(context, target, kind) {
  context.program.stateMap(rowKey).set(target, kind.asNumber());
}

export const $decorators = { FalconBoundary: {
  row: $row,
  godot: (context, target) => context.program.stateSet(godotKey).add(target),
  packed: (context, target, type) => context.program.stateMap(packedKey).set(target, type),
} };
