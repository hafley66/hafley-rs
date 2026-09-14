import { readFileSync, writeFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { compose } from "../../../../claude-research/skills/d2-authoring/theme-kit/1_theme.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = dirname(here);
const names = readdirSync(join(here, "0_sources")).filter((name) => name.endsWith(".d2")).sort();
const rendered = [];
for (const name of names) {
  const { assignments, ...options } = JSON.parse(readFileSync(join(here, name.replace(".d2", ".json")), "utf8"));
  const result = compose(readFileSync(join(here, "0_sources", name), "utf8"), assignments, options);
  writeFileSync(join(root, name), result.source);
  writeFileSync(join(here, name.replace(".d2", ".legend.json")), `${JSON.stringify(result.legend, null, 2)}\n`);
  const svg = join(root, name.replace(".d2", ".svg"));
  const compile = spawnSync("d2", [join(root, name), svg], { encoding: "utf8", timeout: 60000 });
  if (compile.status !== 0) throw new Error(compile.stderr);
  const png = spawnSync("rsvg-convert", ["-w", "1800", "-o", join(root, name.replace(".d2", ".png")), svg], { encoding: "utf8" });
  if (png.status !== 0) throw new Error(png.stderr);
  rendered.push(result.source);
}
let index = readFileSync(join(root, "index.md"), "utf8");
let i = 0;
index = index.replace(/```d2\n[\s\S]*?```/g, () => `\`\`\`d2\n${rendered[i++]}\`\`\``);
if (i !== names.length) throw new Error(`Expected ${names.length} index fences, got ${i}`);
const note = "Edge colors: source groups in ER/flow/predicate diagrams; event phases in the identity sequence; semantic roles in state charts. Dashed logical references are retained. Rebuild inputs and color legends live in `7_theme/`.";
if (!index.includes(note)) index = index.replace("## Implemented ER model", `${note}\n\n## Implemented ER model`);
writeFileSync(join(root, "index.md"), index);
console.log(`Rebuilt ${names.length} graphs and the Instant fence gallery.`);
