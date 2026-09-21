import { chainC } from "./chain_c.js";

export function chainB(text: string): string {
    return chainC(text);
}
