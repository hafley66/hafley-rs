import { readFileSync } from "node:fs";
import { LOG } from "./log";

function pad(raw: string): string { return ` ${raw} `; }

function slug(raw: string): string { return raw.toLowerCase(); }

export function label(raw: string): string { return pad(raw); }

export function loadConfig(dir: string): string {
  LOG("load");
  return pad(slug(readFileSync(dir, "utf8")));
}
