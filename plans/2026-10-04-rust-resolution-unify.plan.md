# Rust module and name resolution: one tasking across graph, rename, cleave, move

Read at origin/main 0390af1d, 2026-10-04. Plan only; nothing built or run beyond short
ripgrep and `ryii graph --callers` reads (release ryii from lab/duckdb-post).

Repo rule this plan serves (CLAUDE.md, "One implementation per concern"): Rust has
exactly two module resolvers, fast = `RustModuleIndex`, slow = rust-analyzer; every
tool asks one of them; no edit arm keeps its own module or `#[path]` reading.

## 1. Inventory: every place that resolves Rust modules, paths or names today

### 1.1 Resolvers and their callers

| location | resolver | tools that use it | reads `#[path]` | crate-name and extern handling | crate and target identity from | rust-analyzer tier |
| --- | --- | --- | --- | --- | --- | --- |
| crates/hafley_scm/src/read/lang/rust_modules.rs:827 `RustModuleIndex` (built at crates/hafley_scm/src/read/project.rs:799) | fast resolver | graph (default, `--callers`, `--uses`, `--from`), every syntax resolve arm (crates/hafley_scm/src/read/lang/rust/2_call.rs, 1_type.rs) | yes, own reading: `normalize_join`, `mod_file` (rust_modules.rs:1735), `path_parents`, `crate_dirs_of` (rust_modules.rs:967) | yes, own reading: `crate_libs` reads each `Cargo.toml` off disk (rust_modules.rs:594), `crate_deps_of` (rust_modules.rs:1136), `known_crate_idents` from directory names | nearest `Cargo.toml` per corpus file (`nearest_crate_dirs`), target roots by path shape (`is_target_root`, rust_modules.rs:988) | none |
| crates/sprefa-extract/src/edit/_7_cleave.rs:1968 `rust_route_index` | fast resolver, a second build of `RustModuleIndex` over the cleave batch texts | cleave (import routes, `declaring_files`, `module_call`) | yes, through `RustModuleIndex` | yes, through `RustModuleIndex` | same as above | none |
| crates/hafley_scm/src/read/lang/8e_rust_checker_modules.rs:32 `module_tree` -> `RustModuleTree::places`, `extern_name`, `sync` | slow resolver, def maps only | cleave (destination gate _7_cleave.rs:778/784, `spell_module`, `declaring_item`, `publish_module` in crates/sprefa-extract/src/edit/rust_mutate.rs:127, 297, 303) | yes, rust-analyzer's own | yes, rust-analyzer dependency entries | `cargo metadata --offline --no-deps` rendered as rust-project.json (crates/hafley_scm/src/read/lang/7a_rust_checker_project.rs) | Names |
| crates/hafley_scm/src/read/lang/rust_checker_ra.rs:159 `answer` (eager slow answer) -> `RustCheckerIndex` | slow resolver, whole-project inference | graph `--rust-checker` (crates/sprefa-extract/src/0_graph.rs:70), graph `--slow` arms other than `--from`, `--call-path`, `--callers`, `--uses` (`slow_project`), the `slow` verb (crates/hafley_scm/src/read/2_slow.rs:661 `checker_facts`) | yes | yes | `cargo metadata` with dependencies, sysroot discovered | Slow |
| crates/hafley_scm/src/read/lang/8f_rust_checker_body_edges.rs:61 `WalkSession::open`, `body_edges`; 8g_rust_checker_walk.rs `demand_walk` | slow resolver, per-body inference | graph `--slow --from`, `--call-path` on a Rust seed (crates/sprefa-extract/src/0c_graph_walk.rs:22 routing) | yes | yes | same as Slow | Slow |
| crates/hafley_scm/src/read/lang/8a_rust_checker_target.rs:103 `target_calls`; 8b_rust_checker_target_types.rs `target_types` | slow resolver, reference search | graph `--slow --callers`, `--uses` (crates/sprefa-extract/src/0a_graph_target.rs:101, 132) | yes | yes | same as Slow | Slow (the tier argument exists; production passes Slow) |
| crates/hafley_scm/src/read/lang/8d_rust_checker_rename.rs:83 `rename` via crates/sprefa-extract/src/edit/1f_ra_rename.rs | slow resolver | rename `--slow` (crates/sprefa-extract/src/edit/rust_rename.rs:60) | yes | yes | same as Slow | Slow |
| crates/hafley_scm/src/read/lang/rust_checker_ra.rs:54 `field_reads` | slow resolver | cleave field widening when `--slow` or a warm Slow host exists (_7_cleave.rs:1149) | yes | yes | same as Slow | Slow |
| crates/sprefa-extract/src/edit/rust_rename.rs:320 `Corpus::open`, :391 `Corpus::resolve`, :2194 `path_module_table`, :2266 `module_path`, :2324 `crate_roots`, :2342 `crate_idents`, :2353 `manifests` | third resolver, private to rename | rename (fast), including the re-export reach for enum variants and field owners (`nameable`, `owner_reach` :884, `variant_leaf` :864) | yes, own reading (`path_attrs` :2174, `path_decls` :2226) | yes, own reading: package identifier from each `Cargo.toml` in the batch | target roots by path shape (`auto_crate_root` :2312) plus `explicit_lib_path` | none |
| crates/sprefa-extract/src/edit/rust_rehome.rs:1711 `crate_roots`, :1459 `module_path`, :670-724 `module_dir`, `decl_base`, `attr_base`, `resolve_decl`; crates/sprefa-extract/src/edit/rust_rehome/cross.rs:542 `module_tree`, :48 `packages`, :80 `cargo_package` | fourth resolver, private to move | move (same package and cross package); cleave also calls `cargo_package` (_7_cleave.rs:1881, 2966; rust_mutate.rs:359 `foreign_crate`) | yes, own reading (`path_attr` rust_rehome.rs:455) | yes, own reading: `[package]`, `[lib]`, `[[bin]]` paths and dependency keys parsed from `Cargo.toml` text | target roots by path shape (`auto_crate_root` rust_rehome.rs:1734) plus manifest target paths | none |
| crates/sprefa-extract/src/edit/rust_mutate.rs:372 `parent_candidates`, :386 `module_name` | layout probe in the cleave arm | cleave, new-file declaration | no | no | rustc probe order by path shape | none |

### 1.2 Rust-analyzer loads per command

`checker_workspace(root, tier, ..)` (crates/hafley_scm/src/read/lang/8_rust_checker_session.rs:45)
caches one host per `(canonical root, tier)` in the process-wide `CHECKER_WORKSPACES`.
`Tier::Fast` has no production caller (ripgrep: only the tests at 8_rust_checker_session.rs:258
and 8a_rust_checker_target.rs:286-289).

| command | fast resolver used | rust-analyzer tiers loaded | rust-analyzer loads |
| --- | --- | --- | --- |
| graph (default) | `RustModuleIndex` | none | 0 |
| graph `--rust-checker` | `RustModuleIndex` | Slow (eager answer) | 1 |
| graph `--slow --from` or `--call-path`, Rust seed | none | Slow (walk session) | 1 |
| graph `--slow --callers` or `--uses` | `RustModuleIndex` (target facts first) | Slow (shared by target calls and target types) | 1 |
| graph `--slow`, any other arm; the `slow` verb | none | Slow (eager answer) | 1 |
| rename | rename's own corpus layout | none | 0 |
| rename `--slow` | none | Slow | 1 |
| cleave | `RustModuleIndex` (second build) and cleave's `cargo_package` | Names | 1 |
| cleave `--slow`, or with a warm Slow host | same | Names and Slow | 2 |
| move | move's own module tree and `cargo_package` | none | 0 |

### 1.3 Readers of `Cargo.toml` and crate identity

| reader | location | input |
| --- | --- | --- |
| fast resolver | rust_modules.rs:594 `crate_libs`, :1136 `crate_deps_of`, :1108 `nearest_crate_dirs` | manifest text read off disk per crate directory |
| rename | rust_rename.rs:2342 `crate_idents`, :2353 `manifests` | manifest text in the batch |
| move and cleave | rust_rehome/cross.rs:48 `packages`, :80 `cargo_package`; rust_rehome.rs:1593 `manifest_leaves` | manifest text in the batch |
| rust-analyzer Names and Fast | 7a_rust_checker_project.rs `fast_project` | `cargo metadata --offline --no-deps --filter-platform host` |
| rust-analyzer Slow | 8_rust_checker_session.rs:226 | `ProjectWorkspace::load` (cargo metadata with dependencies) |

### 1.4 Self-dogfood misses (issues/ryii-dogfood-self-20261003/item.md) mapped to the resolver that answered

| miss | resolver that answered | code reading of the cause (not reproduced in this plan) |
| --- | --- | --- |
| fast graph: `crate::scmpp::write_file` in src/0_query.rs, a file `#[path]`-included into the `ryi` bin, has 0 callers | fast resolver | `crate_module_roots` maps each file to its package directory "for `crate::` anchoring" (rust_modules.rs:880); `crate::` then reads the package, not the bin target whose module tree includes the file |
| fast graph: `use sprefa_extract::{rename_for}` from a bin-included file does not bind | fast resolver | own-crate name binding depends on `crate_libs` (the lib must be in the corpus and its manifest readable off disk) and `sees_path` target scopes built from path shapes |
| fast rename: `tests/195_scmpp_growth.rs` `#[path = "../src/bin/ryi/2_scmpp.rs"] mod scmpp;` site missed | rename's private resolver | `path_module_table` and `module_path` are a separate layout reading; the miss is in rename's own code, so a fix there fixes rename only |
| `graph --uses` misses a type inside a turbofish | syntax extraction, not module resolution | out of scope here |
| (this read) `ryii graph --callers checker_workspace` over crates/sprefa-extract/src and crates/hafley_scm/src returns 1 edge (8e, which imports the name); ripgrep shows 7 production call sites, 6 spelled `super::super::rust_checker_session::checker_workspace` from modules `#[path]`-included under rust_checker_ra.rs | fast resolver | `super` chains through `#[path]` children of a `#[path]` module |
| (this read) `ryii graph --callers crate_roots` binds rename's and move's two `crate_roots` with grade `~` (name match) for 4 of 5 edges | fast resolver | same family |

## 2. Types and algorithms that change

### 2.1 One crate graph, read once, fed to both resolvers

```rust
// crates/hafley_scm/src/read/lang/7_rust_crate_graph.rs (new small file; 7a renders it)
pub struct CrateGraph {
    pub targets: Vec<CrateTarget>,
    pub dependencies: Vec<CrateDependency>,
}
pub struct CrateTarget {
    pub target: TargetId,                // dense index into `targets`
    pub package_dir: PathBuf,
    pub root_file: PathBuf,              // absolute, canonical
    pub kind: TargetKind,                // Library, Binary, Test, Bench, Example, BuildScript, ProcMacro
    pub crate_name: String,              // the name an extern path spells
    pub cfg: Vec<String>,                // every feature on, plus test and debug_assertions
}
pub struct CrateDependency {
    pub from: TargetId,
    pub to: DependencyEnd,               // Target(TargetId) | Registry { package_name: String }
    pub extern_name: String,             // the dependency entry's name, rename included
    pub kind: DependencyKind,            // Normal, Dev, Build
}
pub fn crate_graph(root: &Path) -> Result<CrateGraph, CheckerError>;
// body: cargo metadata --offline --no-deps --filter-platform host (today inside fast_project);
//       a bin, test, bench and example target gets a Normal edge to its own package's library
//       under the library's crate name.
pub(super) fn project_json(graph: &CrateGraph, sysroot: bool) -> ProjectJson; // today's fast_project body
```

Today: four readers derive crate and target identity separately (table 1.3); three of them
guess target roots from path shapes. Proposed: `crate_graph` is the only reader for the
fast resolver, the Names tier, rename and move. The Slow tier keeps `ProjectWorkspace::load`
(it needs dependency sources and the sysroot).
Why: removes rename's `crate_roots`/`crate_idents`/`manifests`, move's `crate_roots`/`packages`
target-root logic, the fast resolver's `is_target_root`/`crate_libs`/`known_crate_idents`; the
own-lib-by-crate-name miss and `[[bin]] path` gaps follow from the shared edge list.

### 2.2 The fast resolver answers module places per target

```rust
// crates/hafley_scm/src/read/lang/rust_modules.rs (split on touch; new pieces in numbered files)
pub struct ModulePlace {                 // one type, today in 8e_rust_checker_modules.rs:13
    pub target: TargetId,                // added; crate_root stays derivable
    pub crate_root: PathBuf,
    pub crate_name: String,
    pub path: Vec<String>,
    pub decl: Option<(PathBuf, u32, u32)>,
}
impl RustModuleIndex {
    pub fn build(graph: &CrateGraph, files: Vec<(String, RustModuleFacts)>,
                 corpus: &[(String, ContentId)], def_index: &DefIndex) -> RustModuleIndex;
    pub fn places(&self, file: &str) -> &[ModulePlace];
    pub fn resolve_prefix(&self, from: &ModulePlace, prefix: &[String]) -> PrefixHead;
    pub fn extern_name(&self, from: TargetId, to: TargetId) -> Option<&str>;
}
pub enum PrefixHead {
    Module(ModulePlace),                 // crate, self, super chains, own-library crate name, workspace dependency
    External { extern_name: String },    // registry dependency or sysroot
    Above,                               // super past the crate root
    Miss,
}
```

Algorithm today: per file, a module path from layout (`module_segments`) overridden by
`#[path]` displacement; `crate::` anchored at the file's package directory; target scopes by
path shape. Proposed: a worklist from each `CrateTarget.root_file` over `mod_decls`
(`#[path]` against the declaring file's directory, else `name.rs`/`name/mod.rs` under the
module directory, inline `mod` chains included), producing every `ModulePlace` a file is, one
per target that reaches it. `crate::`, `self`, `super` and a crate name resolve against the
asking site's `ModulePlace`; a file in several targets answers once per place.
Why: the `#[path]`-in-bin `crate::` miss, the `super::super::` through `#[path]` miss, and the
own-lib-by-name miss share the one module tree; rename and move can then ask it.

### 2.3 One selector between the two resolvers

```rust
// crates/sprefa-extract/src/edit/1h_rust_module_tree.rs (exists; grows)
pub enum RustModules<'a> {
    Fast(&'a RustModuleIndex),           // built over the batch's planned and overlaid texts
    Slow(&'a RustModuleTree),            // rust-analyzer def maps, synced to the same texts
}
pub fn modules(cx: &MoveCx) -> Result<RustModules<'_>, String>;   // --slow or tool default picks
pub fn places(cx: &MoveCx, rel: &str) -> Result<Vec<ModulePlace>, String>;
pub fn resolve_prefix(cx: &MoveCx, from: &ModulePlace, prefix: &[String]) -> Result<PrefixHead, String>;
pub fn extern_name(cx: &MoveCx, from: &ModulePlace, to: &ModulePlace) -> Result<String, String>;
```

`RenameCx` gets the same three functions (rename has its own context type today,
crates/sprefa-extract/src/edit/_1_rename_cx.rs).
Today: cleave asks the Names tier for places and `RustModuleIndex` for routes; rename and move
ask their own code. Proposed: every edit arm calls these functions only.
Why: the repo rule; the rename test-include miss and any move `#[path]` gap become fast-resolver
fixes shared by graph.

### 2.4 Rename's corpus keeps its scan, drops its layout

```rust
struct Corpus {
    scans: BTreeMap<String, FileScan>,
    names: Rc<RefCell<Strings>>,
    homes: BTreeMap<String, Vec<ModulePlace>>,   // from places(), never empty for a reached file
}
impl Corpus {
    fn open(cx: &RenameCx, old: &str) -> Result<Self, RenameStop>;
    fn resolve(&self, cx: &RenameCx, home: &ModulePlace, chain: &[String], prefix: &[String]) -> Option<ModulePlace>;
    // nameable, scope_of, reexports, harvest, owner_reach, variant_leaf: unchanged algorithms over ModulePlace
}
```

Today: `Corpus::open` builds `homes` from `path_module_table` plus `module_path`, and
`Corpus::resolve` handles `crate`, `self`, `super` and crate names itself (rust_rename.rs:391).
Proposed: `homes` from `places`; `resolve` delegates its head to `resolve_prefix` and keeps only
the inline-mod chain arithmetic. The re-export closure (`nameable`, `reexports`) and the
variant/field owner reach from commit f7fd1f84 stay rename's: they are binding of one symbol's
spellings, not module placement. A file under no target keeps today's orphan home only when
the selector says the file is outside every target (`places` empty), and the rename abstains for
sites in it (today it silently resolves orphans against themselves).
Why: the rule; the test-include miss.

### 2.5 Move's planner asks the selector before and after

```rust
// crates/sprefa-extract/src/edit/rust_rehome.rs and rust_rehome/cross.rs
fn here_before(cx: &MoveCx, rel: &str) -> Result<Vec<ModulePlace>, String>;  // places over disk texts
fn here_after(cx: &MoveCx, rel: &str) -> Result<Vec<ModulePlace>, String>;   // places over planned texts
fn cargo_package(cx: &MoveCx, rel: &str) -> Option<CrateTarget>;             // from CrateGraph
```

Today: `crate_roots`, `module_path`, `owning_root`, `module_tree`, `packages` read layout,
`#[path]` and manifests. Proposed: before and after placements come from `places` over the two
text sets (`RustModuleTree::sync` already lays planned texts over the host; the fast index is
rebuilt over the planned texts, a syntax-only cost). Manifest edits (dependency entries,
`[[bin]] path` respelling in `manifest_respell`) stay move's: they write manifests, they do not
resolve modules.
Why: the rule.

### 2.6 Rust-analyzer tiers

```rust
pub enum Tier { Slow, Names }            // Fast removed: no production caller
pub fn module_tree(root: &Path, budget: Duration) -> Result<RustModuleTree, CheckerError>;
// body: a warm Slow host for `root` answers def-map questions when present; else load Names.
```

Today: three tiers, two used in production, cached per `(root, tier)`; cleave `--slow` loads
both. Proposed: drop `Fast` and its test-only branch (or keep it as Names plus the minicore
sysroot if the target-call tests need it; open question 4); `module_tree` reuses a warm Slow
host, so a process that already holds Slow loads one host.
Why: cleave `--slow` goes from 2 loads to 1 when Slow is loaded first; one fewer crate-graph
variant to keep in agreement.

### 2.7 Unchanged

`WalkSession::body_edges`, `demand_walk`, `target_calls`, `target_types`, rust-analyzer rename,
`field_reads` and the eager `answer` keep their algorithms. The slow resolver is already single.
`checker_facts` declines (commit cb404a5e) are unaffected.

## 3. Instance lifetimes

| instance | owner | created | lives until | warm across questions |
| --- | --- | --- | --- | --- |
| `CrateGraph` | `ProjectCx` indexes (resolve), `MoveCx` and `RenameCx` (edit) | first question needing it; one `cargo metadata --no-deps` | end of the question; in the daemon, cached per canonical root keyed by the workspace manifests' content ids | daemon: yes, until a manifest's content id changes |
| `RustModuleIndex` over disk texts | `ProjectCx` (`OnceLock` per refresh) | `resolve_project` | end of the refresh | daemon: per refresh, as today |
| `RustModuleIndex` over planned texts | `MoveCx` (replaces `rust_route_index`'s build) | first `places` call in a batch | end of the batch; rebuilt when the overlay set changes | no |
| rust-analyzer Slow host | `CHECKER_WORKSPACES[(root, Slow)]` | first slow question | process exit; reloaded when a supplied file is outside the load | daemon: yes |
| rust-analyzer Names host | `CHECKER_WORKSPACES[(root, Names)]` | first slow-resolver module question with no warm Slow host | process exit | daemon: yes |
| `RustModuleTree` overlay (`staged`) | `MoveCx` | first `sync` | end of the batch; must release its overlay on drop (today it has no `Drop`, so in the daemon a dry-run cleave can leave planned texts in the shared Names host; read in code, not reproduced) | no |
| `WalkSession` | `demand_walk` call | `WalkSession::open` | end of the walk; the host stays in `CHECKER_WORKSPACES` | host yes, `navs` cache no |
| rename `Corpus` | one rename request | `Corpus::open` | end of the request | no |

Per process: at most one Slow and one Names host per root, as today; after this plan a Names
host is created only when Slow is cold. In the daemon both stay warm for the daemon's life.

## 4. Storage, reads, writes, uniqueness

| fact | produced by | lives in | read by | uniqueness |
| --- | --- | --- | --- | --- |
| crate target (target id, package directory, root file, kind, crate name) | `crate_graph` | memory; option: closed store relation `rust_target` with interned strings | fast resolver, Names project json, rename, move, cleave | one row per (package id, target name, kind) |
| crate dependency (from target, to target or registry package, extern name, kind) | `crate_graph` | memory; option: `rust_target_dependency` | fast resolver `resolve_prefix`, `extern_name`, `sees` | one row per (from target, extern name, kind) |
| module place (file, target, module path, declaring file and range) | fast resolver worklist, or rust-analyzer `file_to_module_defs` | memory; option: `rust_module_place` | rename homes, move before/after, cleave spelling and gates, graph `crate::`/`super`/name heads | one row per (file content id, target id); a file outside every target has zero rows and is reported, never guessed |
| resolved call and type edges | fast resolver arms, eager answer, walk | closed store base tables behind `resolved_edge` and `node` contract views | graph views, sprefa | unchanged |

Write sequence for one fast question: `crate_graph` once, then module facts per file
(syntax), then the module-place worklist from every target root, then export tables on demand
(today's lazy `tables`). For an edit batch: same, over the planned texts, after the planner's
overlay is final for that step. Reads never write back into the index; a new overlay set builds
a new index.

Contract views: `resolved_edge` and `node` keep their columns; no version bump in
crates/sprefa-extract/schema/1_facts.tsp unless the user takes the store option for module
places (open question 3), which adds relations and a version bump with a note to sprefa.

## 5. Ramifications

### 5.1 What changes behaviour

| area | change | expected effect |
| --- | --- | --- |
| fast graph | `crate::`, `super`, own crate name resolve per target place | new edges in bin-included and `#[path]` files (the two dogfood graph misses, the `checker_workspace` callers); graph goldens with `crate::` in bins or tests change |
| fast graph | targets from `cargo metadata` | a corpus without a readable Cargo workspace at `--root` loses crate identity; today path shapes still give some; needs a decline line or a fallback decision (open question 1) |
| fast graph timing | one `cargo metadata --offline --no-deps` per question (daemon: cached) | measured cost needed; the Names tier already pays it |
| rename | homes from `places` | the test-include site appears; orphan-file sites become abstains; tests/5_rename_rust.rs, 146, 147, 176 goldens may move |
| move | placements from `places` | tests/1_move.rs, 2_move_refs.rs, 3_move_rust.rs, 173_move_cross_crate.rs, 42_move_list.rs re-run; any fixture without a Cargo workspace stops with a reason |
| cleave | one selector; `rust_route_index` replaced; Names host reused from Slow | tests/166_cleave_rust.rs, 170, 175 re-run; cleave `--slow` one load fewer when Slow is first |
| `Tier::Fast` removal | tests at 8_rust_checker_session.rs:258 and 8a_rust_checker_target.rs:286 rewrite to Names or Slow | none in production |
| daemon | overlay release on `RustModuleTree` drop; `CrateGraph` cache | warm hosts stop carrying dry-run texts |
| sprefa provider protocol | unchanged for `body_edges`; optional module-place provider (open question 5) | none unless chosen |
| CLI flags | none added; `--slow` on rename and move selects the slow resolver for places | move gains a slow mode it lacks today |

### 5.2 Migration order and risk

| step | content | risk | gate |
| --- | --- | --- | --- |
| first | `CrateGraph` and `crate_graph`; `fast_project` renders from it; `Tier::Fast` removed | low; Names output must match byte for byte | existing Names and walk tests, tests/188_rust_cargo_metadata.rs |
| second | `ModulePlace` moves to the shared type with `target`; fast resolver worklist builds `places`, `resolve_prefix`, `extern_name` beside the old fields; agreement test: fast `places` equals Names `places` on the repo's own crates and every Rust fixture with a manifest | medium; the worklist and rust-analyzer can differ on `cfg`-gated `mod` items (fast sees every `mod`) | new agreement test; tests/185_rust_mod_file_edges.rs, 30_rust_mod_scope_owner.rs, 24_rust_specifiers.rs |
| third | fast graph switches `crate::`, `super`, crate-name heads to `resolve_prefix`; old `crate_module_roots`, `crate_libs`, `known_crate_idents`, `is_target_root` deleted | medium; graph goldens move | dogfood graph misses as regression tests; graph goldens reviewed as diffs |
| fourth | selector in 1h_rust_module_tree.rs; cleave on it; `rust_route_index` gone; overlay release on drop | low to medium | cleave tests |
| fifth | rename on the selector; `path_module_table`, `module_path`, `crate_roots`, `crate_idents`, `manifests`, `module_dir`, `path_attrs` in rust_rename.rs deleted | medium; rename goldens; orphan abstains | dogfood rename miss as a regression test; rename ladder |
| sixth | move on the selector; rust_rehome `crate_roots`, `module_path`, `owning_root`, `module_tree`, `packages` target logic deleted; `cargo_package` returns `CrateTarget` | high; move has the widest fixture set and plans before/after placements | every move test; cross-crate move |
| seventh | `module_tree` reuses a warm Slow host | low | cleave `--slow` load count from tracing span `rust_analyzer.load` |

Each step is one writer and one merge (repo rule). rust_modules.rs (2732 lines), rust_rename.rs
(2363) and rust_rehome.rs (1752) are over the size rule; each step lands new code in new
numbered files and moves what it touches out.

### 5.3 Out of scope

The `--uses` turbofish miss (syntax extraction); the walk's open items (macro expansions,
dyn dispatch, Passed through project fns); replacing the eager answer for `--type-path`,
`--flow-path` and the `slow` verb with demand walks; TypeScript and Go resolvers; the sprefa
program that will drive `body_edges`.

## 6. Alternatives considered

| alternative | why not taken |
| --- | --- |
| Make the Names tier (rust-analyzer def maps) the only module answer for every tool, fast graph included, and shrink `RustModuleIndex` to name binding | fast graph would carry a rust-analyzer load and its memory on every question (today 0 loads); conflicts with the rule naming `RustModuleIndex` as the fast resolver; the decision is the user's (open question 2) |
| Fix each dogfood miss in place: the `crate::` anchor in `RustModuleIndex`, `path_module_table` in rename, move untouched | keeps three private layout readers that already disagree (dogfood: fast graph and fast rename disagree on the same file); the rule says no edit arm keeps its own reading |
| Keep separate readers and add a cross-check test asserting rename, move and fast graph agree on module paths | detects drift, removes none; three implementations of one concern remain |
| Persist module places in the closed store and have rename and move query SQL | adds store writes to every edit batch over planned texts that are not on disk; possible later as the provider seam (open question 3) |

## 7. Open questions

1. With no loadable Cargo workspace at `--root`, the repo rule ("no fallback heuristics") points to declining Rust module heads with a reason; confirm, since fixture corpora without a manifest then lose `crate::` edges they get today.
2. Is the fast resolver allowed to stay a hand-written module tree over `cargo metadata` targets, or should module placement come from rust-analyzer def maps even on the fast path?
3. Should module places and crate targets become closed-store relations with contract views for sprefa, or stay in memory?
4. Remove `Tier::Fast` outright, or keep it for the target-call agreement tests?
5. Should the daemon serve a module-place provider beside `body_edges`?
6. Rename of a site in a file outside every target: abstain (proposed) or resolve against itself as today?
7. Should move gain `--slow` for placements, or always use the fast resolver?
8. Should the `CrateGraph` cache in the daemon key on manifest content ids or on file modification times?

## Decisions (user, 2026-10-04: "yes to all")

| question | decision |
| --- | --- |
| 1 no loadable Cargo workspace at --root | decline Rust module resolution with the reason |
| 2 fast resolver source | rust-analyzer module maps without std (no inference) on the fast path; the hand-written RustModuleIndex retires once agreement is shown (user: "i dont love my hand written rust module index") |
| 3 module places and crate targets | closed-store relations with contract views for sprefa |
| 4 Tier::Fast (rust-analyzer load mode) | remove; rename the rust-analyzer load enum so fast/slow only mean no-types/types |
| 5 daemon module-place provider | yes, beside body_edges |
| 6 rename site in a file outside every target | abstain |
| 7 move --slow | yes |
| 8 daemon CrateGraph cache key | manifest content ids |

Ordering (plans/2026-10-04-ryi-done-rust-typescript.plan.md): warm daemon comes before resolution unify; budgets accepted.
