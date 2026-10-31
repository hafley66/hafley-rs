/** A documented interface. @public */
export interface Documented { /** field doc */ a: number }
/**
 * A documented function.
 * @param x the input
 */
export function documented(x: Documented): number { return x.a; }
/** last one */
export class Last {}
