export interface Result {
  value: number;
}

export interface EnginePort {
  execute(input: Result): Result;
}
