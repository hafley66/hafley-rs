import { join } from "node:path";
export const TEXT_VERSION = 1;
export function here(): string { return join(".", "text"); }
