import { chainB } from "./chain_b.js";

export function chainA(text: string): string {
    return chainB(text);
}
