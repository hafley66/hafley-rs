# ryi on TypeScript: measured utility on hafley-rxjs (2026-10-01)

Binary: `.boop-worktrees/feature/writer-ds2/crates/sprefa-extract/target/release/ryii` (git ff6d647bf6d2, built 2026-10-01T20:12Z). Below, `ryii` means that path.
Corpus: hafley-rxjs @ d0802620, writes done in the worktree `/Users/chrishafley/projects/hafley-rxjs-ryi-dogfood` (branch `dogfood/ryi-ts`, left in place, reset to HEAD).
Scratch: `.dogfood/` inside that worktree (tsconfigs, baselines, the TS LanguageService harness `tsrefs.cjs`, facts.db). Soopy commit state: `/Users/chrishafley/projects/hafley-rxjs-ryi-dogfood.state` (a commit refuses a state root inside the corpus).

Verification method:
- tsc is TypeScript 7.0.2 (tsgo).
- Packages export `dist/*.d.ts`, and the worktree has no build, so each package gets a `.dogfood/tsconfig.<pkg>.json`. It maps every `@hafley66/*` export to its `src` file through `paths`, with `rootDir: ".."`, `typeRoots` set to the package's node_modules, and `composite: false`.
- A pass means zero new `error TS` lines against the baseline taken before the edit.
- The comparison point for tsserver is `ts.LanguageService` from typescript@5.8.2: `findRenameLocations` and `getEditsForFileRename`. It runs over one program of 1,252 files (all `packages/*/{src,tests,test,fixtures,examples,lab}`) with the same `paths` mapping. That is the best case for VS Code, because every consumer is loaded.

## (a) Reach for this when

| Task | Command | What you get | Time measured | Trust level (measured) |
|---|---|---|---|---|
| Rename a function, const, type, or JSX component whose consumers import it by relative path, by tsconfig `paths`, or by a package whose `exports` point at `src` | `ryii rename FILE#NAME NEW --state DIR --text-refs`, then add `--commit` | Per-file diff plan, a stage hash, leftover spellings in docs/markdown | 0.19–0.49 s plan; 0.19–0.27 s commit | mountInView: 46 files, a superset of tsserver's 37, tsc clean in 4 packages. MdPanel: 15/15 locations, matches tsserver. |
| Same rename, but consumers import through a workspace package whose `exports` point at `dist/*.d.ts` (most of hafley-rxjs) | Same command; read the `text-ref` lines | Plan covers only the declaring package; consumers appear only as `text-ref` | 0.36 s | Fails silently. toSignal: 2 of 22 files, exit 0, `abstains: []`, boop-xterm +59 tsc errors. GraphId: 7 of 31 files, +39 tsc errors. |
| Rename a class method | `ryii rename FILE#method NEW --slow --state DIR` | The TS7 LSP rename (tsgo) for the anchor's tsconfig project | 0.28 s | DiagramRenderCache.render: 13/13 matches tsserver, tsc clean, vitest 2/2. Without `--slow`: exit 6 with 286 "untyped property access" lines. |
| Move or rename a file inside a package | `ryii move OLD NEW --state DIR --commit --verify "<tsc cmd>"` | Rewrites static and runtime `import()` specifiers (keeps `.js`, extensionless, and quote style), the moved file's own imports, and package.json `exports` paths; rolls back if `--verify` fails | 0.13–0.42 s; 0.88 s with tsc verify | 2_Signal.ts → core/: tsc clean, vitest 121/121. Misses type-position `import("…")`. Adds a bogus `"hafley-rxjs": "workspace:*"` dependency (D3). |
| Pull one function into a new file | `ryii cleave SRC#ITEM DEST --drag --state DIR --commit --verify "<tsc cmd>"` | New file with its imports; importers repointed | 0.65–1.27 s | longestCodeTokens: tsc clean, vitest 3/3. isSignal: public export lost. installMdviewHost: 2 tsc errors. |
| Find who calls X (quick, name-keyed) | `ryii graph --callers NAME <dir>` | `graph_edge` JSONL with from/to path and line, grade `+` or `~` | 0.09–0.13 s for one package; 0.94–0.99 s for `packages/` | Merges every declaration named NAME. Calls inside closures appear twice. Misses calls to same-file `const f = () =>`. Binds globals to unrelated corpus names. |
| Type references to X | `ryii graph --uses TYPE <dir>` | Declarations that reference TYPE | 0.26 s package; 2.35 s repo | MarkdownTableModel: 10 rows in both tiers |
| Structural grep (tree-sitter, `#eq?`, `#has-ancestor?`) | `ryii query --query 'SCM' --pattern '**/*.ts' --pattern '**/*.tsx' DIR` | JSONL with captures, path, line, end_line | 8.5 s over `packages/` | 474 `.subscribe(` calls; grep finds 529 lines (the gap is multi-line chains reported at their start line). Exit 2 if any input file's grammar lacks the node type. |
| Control dependence of one statement | `ryii graph --slice PATH:BYTE <dir>` | The cfg branch nodes that govern the statement | 0.22 s | Spot-checked once, correct |
| Structural delta between two commits | `ryii diff --from A --to B --pattern '**/*.ts' --pattern '**/*.tsx'` | Added/removed files, edges, imports, type edges | 2.15 s, 416 MB | Closure-offset churn (D19) |
| SQL over all facts | `ryii --resolve --sqlite DB --pattern '**/*.ts' --pattern '**/*.tsx' packages` | 87 tables | 5.76 s, 3,207,058 rows, 522 MB | `occurrence` table is empty under `--resolve` |
| Dependency-first file numbering | `ryii stratify DIR --from DIR/index.ts` | strata, cycles, `move_tsv` proposals | 0.08 s | Phantom cycle (D11); stacked prefixes (D12) |
| Generated region between markers | `ryii region FILE ID [--apply --state DIR]` | Drift report or in-place replace | under 0.1 s | Works on a .ts file; the corpus has 0 markers |
| Live fact stream | `ryii watch --root R --pattern GLOB --receipts PATH` | TSI relation rows, signed ±1 per generation | Snapshot, then a delta per save | A 2-line append produced 574 change rows |

## (b) Results per experiment

### Reads

| Experiment | Command | Rows or answer | Wall seconds | Notes |
|---|---|---|---|---|
| fast, one package | `ryii fast packages/md` | 20,513 records over 159 files | 0.16 | 71 MB RSS. `.d.ts` files are skipped: `src/0_css.d.ts`, `src/style.css.d.ts` |
| fast, whole repo | `ryii fast packages` | 302,327 records over 1,648 paths (ts, tsx, md, json, …) | 1.99 | 726 MB RSS |
| Workspace package import resolution | from `fast packages` | 144 of 486 (file, `@hafley66/*` specifier) pairs resolved | n/a | Resolved only via tsconfig `paths` (gothic, grapht-golden, path-router-lab) or via `exports` that point at src (docs-kit, bewpp). 342 unresolved, including `@hafley66/signals` ×118, `@hafley66/grapht-model` ×82, `@hafley66/signals/react` ×32. Copying `packages/signals/dist` (with .d.ts.map) into the worktree did not change the rename plan. |
| graph --callers render (md) | fast / `--slow` | 50 rows (25 sites) / 26 rows (13 sites) | 0.10 / 0.30 | Slow lacks all 12 `cache.render(` sites in `0_diagramRenderCache.test.ts` (lines 7–33) |
| graph --callers markdownTableModel | fast / slow, md / packages | 11 rows (6 sites) in all four | 0.13 / 0.39 / 0.99 / 1.21 | Fast and slow edge sets are identical apart from `kind` |
| graph --uses MarkdownTableModel | fast / slow | 10 / 10 | 0.26 / 0.34 (md); 2.35 / 2.76 (packages) | fast grades 8 `+` and 2 `~`; slow grades 10 `+` |
| graph --from markdownTableModel | fast | 1 node (`textOf`) | 0.09 | Missing: `sectionsOf`, `rectangular` (same-file arrow consts called directly) |
| graph --call-path / --type-path | fast | 1 path each | 0.13 / 0.20 | |
| graph --from, --call-path, --type-path with `--slow` | slow | 0 results | 0.15–3.2 | stderr: `ryi slow: no ts checker in this build (cargo feature ts-checker)`. Over `packages`: 0 results with no message. |
| graph --slice | `--slice packages/md/src/lib/1_tableModel.ts:<byte of return codeRunsOf(...)>` | 4 cfg nodes (3 branch, 1 ret) | 0.22 | |
| graph --flow-path | `PATH@3621:3640` and `<git blob>@3621:3640` (span of the `children` param) | 0 paths | 0.13 / 0.23 | Input format is undocumented; no result obtained |
| query, no pattern | `ryii query --query '(call_expression …)' packages` | exit 2, `Invalid node type "call_expression"` | 0.09 | Markdown files in the input set |
| query, ts+tsx globs | `--pattern '**/*.ts' --pattern '**/*.tsx'` | 474 matches (456 ts, 18 tsx) | 8.53 | `--pattern '*.ts'` (no `**/`) gives 18 matches in 1.3 s |
| query, `--lang ts` | every file parsed as TS | 529 matches | 14.2 | |
| query with `#has-ancestor? @call arrow_function` | | 408 matches | 11.0 | `#not-has-ancestor?` evaluated (0 matches for the self-ancestor case) |
| stratify signals | `ryii stratify packages/signals/src --from packages/signals/src/index.ts` | 42 locality, 43 stratify_move, 13 stratum, 1 stratum_cycle, 29 unreached | 0.08 | |
| diff | `--from HEAD~1 --to HEAD` (12 files, +570/−8) | 241 records: 6 files added, 3 changed, 33 edges added, 6 edges removed, 17 imports added, 5 type edges added, 170 unresolved added | 2.15 | Of the 6 removed edges, at least 3 are re-added under a new `closure@N` name |
| watch | `--pattern 'packages/trace/**/*.ts'`, append 2 lines to `trace/src/0_types.ts` | snapshot 4,763 rows; delta 574 rows (278 −1, 296 +1) | n/a | The whole file is retracted and re-asserted |
| sqlite | `--resolve --sqlite .dogfood/facts.db` | 18,612 resolved_edge; 36,631 unresolved; 0 occurrence | 5.76 | |

SQL questions over facts.db:

| Question | SQL (abbreviated) | Answer |
|---|---|---|
| Most cross-file callers | `resolved_edge where caller_path<>callee_path and kind<>'import_resolve' group by callee` | Top 2: `URL` → `vitest-playwright/tests/0_bootstrap.ts` (64 files) and `requestAnimationFrame` → `trace/src/11_performanceReadout.test.ts` (52 files). Both are globals bound by `corpus_unique` (D10). Then `runWhenInView` 38, `grid` 32, `render` 26, `Signal` 18. |
| Unresolved call reasons | `unresolved where family='call' group by reason` | inferred 36,190; spread-call-args 191; ambiguous 180; computed-member-call 69; dynamic-import 1 |
| Resolution origin mix | `resolved_edge group by kind, resolution_origin` | import_resolve/module_plane 6,923; name_resolve/same_file 5,642; name_resolve/module_plane 4,476; name_resolve/receiver 672; name_resolve/corpus_unique 455; value_ref/same_file 253; value_ref/module_plane 191 |

### Writes

| Experiment | ryi plan (files / edits) | tsserver LanguageService | ryi wall seconds | tsc result after `--commit` | Tests |
|---|---|---|---|---|---|
| rename `toSignal` → `asSignal` (signals/src/2_Signal.ts; exported via `export *` and consumed via `@hafley66/signals`) | 2 files / 4 edits; 69 text-ref lines (20 boop-xterm .ts files, plus docs) | 22 files / 70 lines | 0.36 (`--slow` 0.24, same plan) | signals 0 new; boop-xterm +59 new (TS2724 `has no exported member 'toSignal'` and follow-ons) | n/a |
| rename `mountInView` → `mountWhenVisible` (docs-kit; `exports` → src; `.ts` specifiers) | 46 files / 102 lines | 37 files / 84 locations; all inside ryi's set. ryi's 9 extra files are `signal-grid/demo/*`, `signal-grid/site/parity.ts` (outside the TS program) | 0.49 plan / 0.27 commit | docs-kit, signal-grid, signals, signal-marbles: 0 new | `17_in_view.test.ts` is excluded by the vitest config |
| rename method `DiagramRenderCache.render` → `renderCached` | fast: exit 6, 286 "untyped property access reaches the symbol at runtime" lines (every `.render(` in the corpus). `--slow`: 2 files / 13 edits. `--at` is not needed with `--slow`. | 2 files / 13 | 0.28 | md 0 new. The parameter also named `render` was left untouched (correct). | vitest `0_diagramRenderCache.test.ts` 2/2 |
| rename type `GraphId` → `GraphKey` (grapht-model; consumers via `@hafley66/grapht-model` → dist) | 7 files (grapht-model 5, grapht-golden 2 via tsconfig paths); 355 text-ref lines | 31 files / 270 (grapht 174 locations) | 0.46 | grapht +24, grapht-golden +14, scene +1 | n/a |
| rename component `MdPanel` → `MarkdownPanel` (`export const MdPanel = SignalReact(function MdPanel …)`; JSX and `createElement` uses; tests) | 7 files / 15 | 7 files / 15 (identical) | 0.37 | md 0 new. Inner function name left as `MdPanel` (same as tsserver). | n/a |
| rename component `DiagramLightbox` → `DiagramModal` | 6 files | 7 files (adds `boop-xterm/src/8g_diagramOverlay.ts` via `@hafley66/md`) | 0.44 | md 0 new; boop-xterm +1 (TS2724) | n/a |
| rename default export `MarkdownTable` → `MdTable` | 1 file / 1 edit (declaration only) | 4 files / 19 (also renames the local default-import bindings) | under 0.5 | md 0 new | n/a |
| move `signals/src/2_Signal.ts` → `src/core/2_Signal.ts` (18 importers; `.js` and extensionless specifiers; package `exports["./Signal"]` → dist) | 23 files: 21 src + the moved file's own imports + package.json. `exports["./Signal"]` updated to `./dist/core/2_Signal.{d.ts,js}`. Also adds dependency `"hafley-rxjs": "workspace:*"`. | 21 files / 22 edits (no package.json edit) | 0.42 plan / 0.13 commit | signals, md, boop-xterm: 0 new | signals vitest 14 files, 121/121 pass (after copying trace/path dist from main for runtime resolution) |
| move `md/src/ports.ts` → `src/host/0_ports.ts` (32 importers) | 34 files (33 + package.json bogus dep) | 33 files / 35 edits | 0.22 | dry-run only | n/a |
| move `boop-xterm/src/8i_turnPanel.ts` → `src/panels/` | 6 src files + package.json (bogus dep) | 6 files / 9 edits | under 0.5 | boop-xterm +2 TS2307: `9_view.ts:56` and `8j_agentSquares.browser.test.ts:33`, both `Signal<import("./8i_turnPanel.js").TurnPanelTarget …>` | n/a |
| move `vitest-telemetry/.../adapter/navTree.ts` → `.../nav/navTree.ts` | 11 files + package.json (bogus dep) | 11 files / 11 edits (includes `typeof import('./adapter/navTree.js')` at model.test.ts:11; does not touch `vi.mock('./adapter/navTree.js')` at :10) | 0.88 with `--verify` | `--verify "tsc -p …"` caught TS2307 at model.test.ts:11 → `verify failed (rc=1): rolled back 11 files`, exit 3. Leaves an empty `nav/` directory. | n/a |
| move `mmd/src/2_identity.ts` → `src/id/` | runtime `await import("./2_identity.js")` rewritten, plus 4 static sites | not run | under 0.5 | not run | n/a |
| move `gothic/src/kit/0_inputs.ts` → `src/inputs/` (`exports["./inputs"]` → src) | 9 src files + package.json (`exports["./inputs"]` updated, no bogus dep) | 9 files / 10 edits | under 0.5 | dry-run only | n/a |
| cleave `isSignal` from 2_Signal.ts → `2a_isSignal.ts` | 2 files; adds `import { isSignal } from "./2a_isSignal";` to 2_Signal.ts | n/a | 0.65 | signals 0 new; signal-grid +8 and docs-kit +3 (`@hafley66/signals` no longer exports `isSignal`: `index.ts` only has `export * from "./2_Signal.js"`) | n/a |
| cleave `installMdviewHost` from md/src/ports.ts → `0_hostInstall.ts` (with and without `--drag`) | 19 importers repointed; `let host` in ports.ts becomes `export let host`; the new file imports `host` and assigns to it | n/a | 0.75 | md +2: TS2632 `Cannot assign to 'host' because it is an import` (0_hostInstall.ts:5); TS2305 `index.ts(16,10)` re-export `export { installMdviewHost, … } from "./ports.js"` not updated | n/a |
| cleave `longestCodeTokens` → `lib/1a_codeTokenWidths.ts --drag` | 4 files. `codeRunsOf` exported (it has other users), not dragged. Combined imports split into one import per specifier. | n/a | 1.27 | md 0 new | vitest `1_tableModel.test.ts` 3/3 |
| region | `printf … \| ryii region f.ts exports` → `{"status":"drift"}` exit 1; with `--apply` → `"applied"` exit 0 | n/a | under 0.1 | n/a | n/a |
| dismantle | `ryii dismantle packages/md/src/lib/1_tableModel.ts#CodeToken` | `dismantle: TODO, not implemented`, exit 2 | 0.02 | n/a | n/a |

## (c) Defects

Every repro runs from `/Users/chrishafley/projects/hafley-rxjs-ryi-dogfood` with `R=<ryii path>`, `S=--state /Users/chrishafley/projects/hafley-rxjs-ryi-dogfood.state`, and `RUST_LOG=warn`.

| Id | Repro (one line) | Expected | Actual |
|---|---|---|---|
| D1 | `$R rename packages/signals/src/2_Signal.ts#toSignal asSignal $S --json` | 22 files (all `@hafley66/signals` consumers), or a non-zero exit/abstain for unbound consumers | 2 files; 20 consumer files appear only as `text-ref` when `--text-refs` is passed; exit 0, `{"abstains":[]}`. Same with `--slow` (the tsgo LSP loads only `packages/signals/tsconfig.json`) and with signals `dist/` present. Also: GraphId 7 of 31 files; DiagramLightbox 6 of 7. |
| D2 | `$R fast packages \| jq 'select(.record=="resolved_import" and (.name\|startswith("@hafley66/")))'` | Workspace package specifiers resolve via `exports` types → src (or `.d.ts.map`) | 342 of 486 (file, specifier) pairs unresolved; only `paths` and `exports` that point at src resolve. This is the root cause of D1. |
| D3 | `$R move packages/md/src/ports.ts packages/md/src/0_ports.ts $S` | No dependency edits | `dep packages/md/package.json: + "hafley-rxjs": "workspace:*"` (the root package). Reproduced for signals 0_types/1_SignalCreator/2_Signal, md plugins/0_types, signal-grid 0_types, boop-xterm 8i_turnPanel, vitest-telemetry navTree. Not for trace/src/{index,0_types}.ts, grapht-model/src/6_graph.ts, gothic/src/kit/0_inputs.ts. |
| D4 | `$R move packages/boop-xterm/src/8i_turnPanel.ts packages/boop-xterm/src/panels/8i_turnPanel.ts $S --commit` | Rewrites `import("./8i_turnPanel.js")` in type position (9_view.ts:56, 8j_agentSquares.browser.test.ts:33) | Left unchanged → 2× TS2307. Same for `typeof import('./adapter/navTree.js')` (vitest-telemetry model.test.ts:11). Runtime `await import()` is rewritten. |
| D5 | `$R cleave packages/signals/src/2_Signal.ts#isSignal packages/signals/src/2a_isSignal.ts $S --commit` | Public API keeps `isSignal` (re-export from 2_Signal.ts, or index.ts gains the new file) | `@hafley66/signals` drops `isSignal` → signal-grid +8 and docs-kit +3 tsc errors |
| D6 | `$R cleave packages/md/src/ports.ts#installMdviewHost packages/md/src/0_hostInstall.ts $S --commit` | `index.ts:16` `export { installMdviewHost, … } from "./ports.js"` repointed | Not in the plan → TS2305 |
| D7 | Same command as D6, with or without `--drag` | Drag `let host` along, or refuse | `let host` becomes `export let host`; the new file assigns to an imported binding → TS2632 |
| D8 | Any cleave above | New import lines follow file style (`.js` suffix; no semicolons in signals) | `import { isSignal } from "./2a_isSignal";` (extensionless, with semicolon); one `import` per specifier (`2_MarkdownTable.tsx` 1 line → 4 lines) |
| D9 | Write `function f(){}` `const g=()=>2` `const k=function(){}` `export function h(){return f()+g()+k()}` to one file; run `$R graph --callers g <dir>` | 1 edge (h → g) | 0 edges for `g` and `k`; `f` gets 2. In the corpus: `$R graph --callers sectionsOf packages/md` → 0, though `1_tableModel.ts:92` calls it. |
| D10 | `$R graph --callers requestAnimationFrame packages/signal-grid packages/trace` | 0 corpus edges (it is a DOM global) | 38 `~` edges into object-literal keys in `trace/src/11_performanceReadout.test.ts`. Over `packages`, `corpus_unique` binds 455 edges across 44 names, e.g. `URL` → `export const URL` in `vitest-playwright/tests/0_bootstrap.ts` (60 caller files), and `invalidate()` in `signals/src/1_SignalCreator.ts:532,615` (a local `const invalidate` at :625) → the type member `invalidate` in `4_Query.ts` |
| D11 | `$R stratify packages/signals/src --from packages/signals/src/index.ts` | Cycle {3_Endpoint, 4_Query} (static imports) | `stratum_cycle` {1_SignalCreator, 2_Signal, 3_Endpoint, 4_Query}, from the D10 edge 1_SignalCreator → 4_Query.invalidate |
| D12 | Same as D11 | Replace the numeric prefix (`10_slice.ts` → `2_slice.ts`) | Stacks prefixes: `0_log.ts` → `0_0_log.ts`, `10_slice.ts` → `2_10_slice.ts`, `2_Signal.ts` → `1_2_Signal.ts` |
| D13 | `$R graph --callers markdownTableModel packages/md` | One row per call site | 11 rows for 6 sites. Sites inside a closure are emitted twice: once as `<module>` or the named enclosing function (`name_resolve`), once as `closure@N` (`import_resolve`). The site at `2_MarkdownTable.tsx:164` (no closure) appears once. Both tiers. Refutes "every edge twice". |
| D14 | `$R graph --callers render --slow packages/md` | Includes 12 `cache.render(` sites in `src/lib/0_diagramRenderCache.test.ts` (lines 7,8,10,11,12,14,15,28–31,33) | 0 of those 12; fast finds all 12. Confirmed. (`rename --slow` on the same method does find them.) |
| D15 | `$R graph --from markdownTableModel --slow packages/md` | Edges, or a non-zero exit | `ryi slow: no ts checker in this build (cargo feature ts-checker)`, exit 0, 0 edges. Same for `--call-path`. Over `packages`: no message, 0 results. |
| D16 | `$R graph --callers 'packages/md/src/lib/0_diagramRenderCache.ts#render' packages/md` | Edges for that one declaration (or a usage error) | `0 edges`, exit 0. Same for `DiagramRenderCache.render`. Bare `render` merges 4 declarations (25 sites). |
| D17 | `$R rename packages/md/src/lib/0_diagramRenderCache.ts#render renderCached $S` | Plan of 13 edits (the receiver type is a local `new DiagramRenderCache()`) | exit 6, 286 lines of `untyped property access reaches the symbol at runtime` covering every `.render(` in the repo. `--slow` is required. |
| D18 | `$R query --query '(call_expression) @c' packages/md` | Matches from .ts/.tsx; other grammars skipped | exit 2, `Invalid node type "call_expression"`, 0 output (md/json/markdown files in the dir) |
| D19 | `$R diff --from HEAD~1 --to HEAD --pattern '**/*.ts' --pattern '**/*.tsx'` | No edge change for unchanged calls | `6_graphRenderer.ts` `closure@16885 → sameCamera` removed plus `closure@17659 → sameCamera` added (same for hoverOpacity and graphHoverColor); the git diff touches none of those calls |
| D20 | `$R move …/adapter/navTree.ts …/nav/navTree.ts $S --commit --verify "<tsc that fails>"` | Rollback restores the tree exactly | Files restored; an empty `nav/` directory is left |
| D21 | `$R --resolve --sqlite db --pattern '**/*.ts' packages; sqlite3 db 'select count(*) from occurrence'` | Same occurrence rows as `fast` (53,684 in `packages` JSONL) | 0 |
| D22 | `$R rename … --state <dir inside corpus> --commit` | Dry-run and commit accept the same state root | Dry-run accepts it; commit fails with `open commit engine: commit state root must be outside target root` |
| D23 | `$R dismantle packages/md/src/lib/1_tableModel.ts#CodeToken $S` | Plan | `dismantle: TODO, not implemented`, exit 2 (help text also says TODO) |
| D24 | `$R fast packages/md \| jq -r .path \| sort -u` | Includes `src/0_css.d.ts`, `src/style.css.d.ts` | Both absent (161 .ts/.tsx on disk; 159 extracted) |
| D25 | `$R graph --flow-path packages/md/src/lib/1_tableModel.ts@3621:3640 packages/md` (and the `<blob>@…` form) | Paths from the `children` param | `0 paths`. BLOB format is not documented, so wrong usage is possible. |

## (d) Gaps against tsserver / VS Code

Baseline: the TS 5.8 LanguageService over one 1,252-file program with all workspace packages mapped to src. Cold, each query took 2.5–3.6 s at 1.05 GB RSS. The ryi rename/move plan took 0.13–0.49 s.

| Case | ryi | tsserver (measured harness) | Evidence |
|---|---|---|---|
| Consumers via `@scope/pkg` whose `exports` point at `dist/*.d.ts` | Misses (fast and `--slow`); lists them as `text-ref` | Finds (with all packages in one program) | toSignal 2 vs 22 files; GraphId 7 vs 31; DiagramLightbox 6 vs 7 |
| Consumers via tsconfig `paths` | Finds | Finds | GraphId: grapht-golden included |
| Consumers via `exports` → src (docs-kit) | Finds | Finds | mountInView: 46 ⊇ 37 |
| Files outside every tsconfig (`demo/`, `site/`) | Included | Not in the program | mountInView +9 files |
| `export *` barrels | Followed | Followed | mountInView through `docs-kit/src/index.ts` |
| Default exports | Renames the declaration only | Also renames local default-import bindings | MarkdownTable 1 vs 19; both tsc clean |
| JSX and `createElement` uses | Found | Found | MdPanel 15/15 |
| Test files (`*.test.ts`, `*.browser.test.tsx`) | Found | Found | MdPanel, render, mountInView |
| Class methods | Fast abstains (exit 6); `--slow` matches | Found | render 13/13 with `--slow` |
| Runtime `await import("./x.js")` on move | Rewritten | Rewritten | mmd 1_parse.test.ts:315 |
| Type-position `import("./x.js")` on move | Missed | Rewritten | turnPanel 2 sites; navTree 1 site |
| `vi.mock('./x.js')` string on move | Not rewritten | Not rewritten | vitest-telemetry model.test.ts:10 |
| package.json `exports` on move | Rewritten (dist and src targets) | Not edited | signals `./Signal`, gothic `./inputs` |
| Markdown/docs mentions | Listed with `--text-refs` | None | toSignal docs lines 504–550 |
| Specifier style on move | Keeps `.js`, extensionless, and quote style | `importModuleSpecifierEnding` setting | 2_Signal move: 11 `.js`, 9 extensionless kept |
| Atomic apply with check | `--verify CMD` rolls back on failure | None | navTree: rolled back 11 files |
| Move one declaration to a new file | `cleave` (drops re-exports; see D5–D8) | "Move to a new file" refactor (not measured) | n/a |
| Call graph | Name-keyed, syntax-only; see D9, D10, D13 | Call hierarchy, by symbol | n/a |
