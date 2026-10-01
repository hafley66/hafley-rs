// `node 3_report.mjs`: run 0_gen.mjs in full mode (every named kind) over every grammar hafley_scm
// depends on, plus npm C; typecheck the outputs (out/gen/<lang>/) and print one markdown row per grammar.
import { execFileSync } from "node:child_process";
import { dirname, join, relative } from "node:path";
import { cargoGrammars, generate } from "./0_gen.mjs";

const here = dirname(new URL(import.meta.url).pathname);
const stats = [];
for (const lang of ["c", ...[...cargoGrammars().keys()].sort()]) {
  try {
    stats.push(await generate(lang, { full: true, outDir: join(here, "out/gen", lang) }));
  } catch (e) {
    stats.push({ lang, error: String(e.message).split("\n")[0] });
  }
}
const files = stats.filter((s) => s.path).map((s) => relative(here, s.path));
let tsc = "";
try {
  execFileSync("node_modules/.bin/tsc", ["--noEmit", "--strict", "--skipLibCheck", "--jsx", "preserve", "--jsxImportSource", "@alloy-js/core",
    "--module", "NodeNext", "--moduleResolution", "NodeNext", "--target", "es2022", "--lib", "es2023", ...files], { cwd: here, encoding: "utf8" });
} catch (e) { tsc = String(e.stdout); }
const tscErrors = (p) => tsc.split("\n").filter((l) => l.startsWith(relative(here, p) + "(")).length;

const COLS = ["lang", "source", "generator_runs", "components", "leaf_components", "never_components", "rules", "hidden_rules",
  "hidden_rules_emitted", "alias_count", "externals", "hidden_external_refs", "locals_scm_shipped", "generated_lines", "tsc_errors"];
console.log(`| ${COLS.join(" | ")} |\n|${COLS.map(() => "---").join("|")}|`);
for (const s of stats) {
  const row = s.error ? [s.lang, "", `no: ${s.error}`, ...COLS.slice(3).map(() => "")]
    : [s.lang, s.from, "yes", s.components, s.leaves, s.pruned, s.rules, s.hiddenRules, s.hiddenRefs, s.aliases, s.externals,
      s.hiddenExternals, s.localsShipped ? "yes" : "no", s.lines, tscErrors(s.path)];
  console.log(`| ${row.join(" | ")} |`);
}
