import { LOG } from "./log";

export const UTIL_VERSION = 1;

function tidy(raw: string): string { return raw.trim(); }

function slug(raw: string): string { return tidy(raw).toLowerCase(); }

export function loadConfig(dir: string): string {
  LOG("load");
  return slug(dir);
}
