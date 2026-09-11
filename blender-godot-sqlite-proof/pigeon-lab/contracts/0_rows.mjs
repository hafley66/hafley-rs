export const rowKey = Symbol.for('pigeon-boundary.row');
export const godotKey = Symbol.for('pigeon-boundary.godot');
export const packedKey = Symbol.for('pigeon-boundary.packed');

export function $row(context, target, kind) {
  context.program.stateMap(rowKey).set(target, kind.asNumber());
}

export const $decorators = { PigeonBoundary: {
  row: $row,
  godot: (context, target) => context.program.stateSet(godotKey).add(target),
  packed: (context, target, type) => context.program.stateMap(packedKey).set(target, type),
} };
