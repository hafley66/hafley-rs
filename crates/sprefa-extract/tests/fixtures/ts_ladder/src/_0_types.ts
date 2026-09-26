export interface Base {
  label: string;
}

export interface Box<T> {
  value: T;
}

export class Service {
  ping(input: Base): Base {
    return input;
  }
}

export function makeService(): Service {
  return new Service();
}
