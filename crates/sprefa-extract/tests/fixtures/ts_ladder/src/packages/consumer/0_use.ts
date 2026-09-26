import type { EnginePort } from "ladder-engine/0_types.ts";

export interface Holder {
  readonly port: EnginePort;
  forward(port: EnginePort): EnginePort;
}

export class Runner {
  constructor(private readonly port: EnginePort) {}
}

export const adapter = {
  use(port: EnginePort): EnginePort {
    return port;
  },
};

export function invoke(port: EnginePort): void {
  port.execute({ value: 1 });
}

export function invokeHolder(holder: Holder): void {
  holder.port.execute({ value: 2 });
}
