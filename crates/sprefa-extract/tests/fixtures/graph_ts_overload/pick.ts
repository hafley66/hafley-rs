// dist/ is tsc 7.0.2 --declaration --declarationMap of this file; consumer/ reads it.
export function pick(value: string): string;
export function pick(value: number): number;
export function pick(value: string | number): string | number {
    return value;
}

export function pickText<T>(text: T): T {
    return pick(text as string) as T;
}
