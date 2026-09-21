import { banner, loadConfig, slug } from "./utils";
export function boot(dir: string): string { return banner(slug(loadConfig(dir))); }
