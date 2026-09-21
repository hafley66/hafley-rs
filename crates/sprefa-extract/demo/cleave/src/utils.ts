import { readFileSync } from "node:fs";
import { join } from "node:path";
import { LOG } from "./log";

const SEP = "-";

function tidy(raw: string): string { return raw.trim(); }

export function describe(raw: string): string { return "<" + raw + ">"; }

export function slug(raw: string): string { return tidy(raw).toLowerCase(); }

export function loadConfig(dir: string): string {
  LOG("load");
  return readFileSync(join(dir, "config.json"), "utf8");
}

export function banner(name: string): string { return SEP + name + SEP; }
