import { loadConfig, slug } from "./util";
export function boot(dir: string) { return slug(loadConfig(dir)); }
