import { readFileSync } from "node:fs";
import { join } from "node:path";
import { LOG } from "./log";

export function slug(raw: string): string { return raw.toLowerCase(); }

export function loadConfig(dir: string): string {
  LOG("load");
  return readFileSync(join(dir, "config.json"), "utf8");
}
