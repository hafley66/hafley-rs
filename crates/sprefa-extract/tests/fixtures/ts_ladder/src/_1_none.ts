export interface Independent {
  value: number;
}

export function independent(value: number): number {
  return value;
}

export class Service {
  other(): void {}
}
