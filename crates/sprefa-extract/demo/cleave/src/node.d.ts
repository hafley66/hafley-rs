declare module "node:fs" {
  export function readFileSync(path: string, encoding: string): string;
}
declare module "node:path" {
  export function join(...parts: string[]): string;
}
