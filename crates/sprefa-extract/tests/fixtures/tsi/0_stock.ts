const astral = "🦀"; export class Base { id: number = 0; }
export class Derived extends Base { extra: string = ""; }
export const derived: Derived = new Derived();
export interface Unrequested { omitted: boolean; }
