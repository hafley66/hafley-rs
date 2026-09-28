type Dependency = { n: number };
function helper(value: Dependency): number { return value.n }
export function run(value: Dependency): number { return helper(value) }
export const kept = helper({ n: 2 });
