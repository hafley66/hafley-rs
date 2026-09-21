import { label, loadConfig } from "./util";
export function boot(dir: string) { return label(loadConfig(dir)); }
