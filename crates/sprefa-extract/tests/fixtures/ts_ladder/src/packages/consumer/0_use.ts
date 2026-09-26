import type { EnginePort } from "ladder-engine/0_types.ts";

export interface Holder {
  readonly port: EnginePort;
}

export function invoke(port: EnginePort): void {
  port.execute();
}

export function invokeHolder(holder: Holder): void {
  holder.port.execute();
}
