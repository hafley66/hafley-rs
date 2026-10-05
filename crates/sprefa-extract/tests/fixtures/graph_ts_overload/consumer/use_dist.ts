import { pick } from "pick-dist";

export function useDist(): string {
    return pick("text");
}
