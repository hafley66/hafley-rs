# ryi CLI cleanup: turnkey fast and slow over one input model

Supersedes `2026-09-25-ryi-inputs.md`. Every fact below comes from a full read of
`src/bin/ryi.rs`, `src/bin/ryi/0_cli.rs`, `src/bin/ryi/0_sqlite.rs`,
`src/0_graph.rs`, `src/0_query.rs`, `src/project.rs`, `src/lang/{2,3,6,7,8}_*.rs`,
`schema/*.tsp`, hafley_scm core, and six reader reports over the language files.

## 1. Current surface (measured)

### 1.1 Verbs and how each takes files

| verb | input field | accepts | expansion code |
| --- | --- | --- | --- |
| `ryi PATH...` (root) | `FileArgs.paths: Vec<PathBuf>` | files; a directory exits 2 | `check_file_paths` `ryi.rs:225` |
| `ryi fast` | `FastArgs.paths` | files; converted to `FileArgs` with `--family diet_scip` (`ryi.rs:77`) | same |
| `ryi slow` | `SlowArgs.root: PathBuf` | one directory; converted to `FileArgs` with `--family scip` (`ryi.rs:89`) | `scip_ensure` walk |
| `ryi graph` | `GraphArgs.paths` | files + dirs, no .gitignore | `expand_paths` `0_graph.rs:292` (`read_dir`) |
| `ryi graph --at REV` | same | repo-relative paths | soopy `RevisionReader` + `watch::default_patterns()` |
| `ryi query` | `QueryArgs.path: PathBuf` | exactly one file, `--lang` required | none |
| `ryi watch` / `ryi diff` | positional ROOT + `--pattern GLOB` | glob over a repo | soopy `SourceTree::enumerate` (gitignore-aware `WalkBuilder` + `globset`) |
| `ryi move` / `rename` / `cleave` | `--root` (default git root) | whole tree | `MoveCx::open` / `RenameCx::open` (`WalkBuilder`, ignore rules off, `SKIP_DIRS`) |
| fast's scm rows | `scm_facts(paths)` | files + dirs | `expand` + `walk` `7_scm_rows.rs:319` (`read_dir`) |

Four hand walkers (`0_graph.rs:292`, `7_scm_rows.rs:334`, `move_cx.rs:45`, `rename_cx.rs:46`) sit beside soopy's gitignore-aware one.

### 1.2 Root command modes

The root command selects one of eight modes with booleans and `--family`:

| flag | mode | code |
| --- | --- | --- |
| (none) | per-file extraction, exactly one PATH unless `--sqlite` | `extract_file` `ryi.rs:536` |
| `--family diet_scip` | fast | `diet_scip*` `project.rs:1305` |
| `--family scip` | slow | `stream_scip_family` `ryi.rs:162` |
| `--resolve` | project resolve, arms from `--family` | `stream_resolve` `ryi.rs:634` |
| `--deps` | TS file_edge rows from syntax | `diet_file_edges_jsonl` |
| `--scip-deps` | file_edge rows folded from SCIP | `scip_file_edges_jsonl` |
| `--package-deps` | manifest edges | `package_edges_jsonl` |
| `--scip-facts` | SCIP passthrough rows | `scip_facts_jsonl` |
| `--ingest` / `--schema` / `--trail` | TSI re-emit / print contract / print run trail | `ryi.rs:416-449` |

### 1.3 Flags with more than one meaning

| flag | meanings |
| --- | --- |
| `--family` | per-file mask `cst,type,call,df,data,cfg` (`parse_mask` `ryi.rs:694`); project mode `scip`/`diet_scip` (`family_mode` `ryi.rs:119`); resolve arms `call,type,flow` under `--resolve` (`parse_arms` `ryi.rs:673`); mask in `watch`; arms in `diff` |
| root dir | `--project-root` (root, graph); positional ROOT (slow, watch, diff); `--root` (move, rename, cleave) |
| `--state` | `graph`: dir holding `graph.db`; `watch`: receipt store; `move`/`rename`/`cleave`/`region`: soopy state root |
| DB output | `--sqlite PATH` (root, fast, slow, diff) vs `--state DIR` (graph) |
| `--json` | graph: drop stderr summary; cleave/rename: closing plan line; diff: no-op |
| checker | `--rust-checker`, `--ts-checker`, `--go-checker` on root + graph, absent on fast/slow; the installed build (`--features cli`) compiles none, `answer()` returns `NotBuilt`, the run warns and exits 0 |
| language | `query --lang` required; `slow --indexer` optional (else `detect()` by markers); everything else routes by `source_for(path)` |

### 1.4 Adjacent output defects seen by all six readers

- `graph --json` still writes ~110 INFO `extract_file` span lines to stderr (default `RUST_LOG sprefa_extract=info`).
- `graph_edge` from `--callers`: `from_name` is the asked-for callee, `to_name` the caller.
- `graph_node.line` is null on every `--from` row.
- `query` rows carry no path; two files in one call fail with `error: unexpected argument 'src/0_move.rs' found`.
- `graph src --call-path diet_scip` (sprefa-extract) ran 8m32s at ~93% CPU with no output until killed.
- `graph --from scm_edges` omits `Store::resolve` (method call) and includes `types.rs parse` (name-matched from tree-sitter's `parser.parse`).

`--callers` recall gaps (method call on `Option<&Index>`, fn-as-value, trait-object dispatch, cross-crate, same-name merge) are resolver quality, tracked by the ratchet, and out of scope here.

### 1.5 Module plane (what an entry crawl reads)

| language | index | file edge today |
| --- | --- | --- |
| ts/js | `TsModuleIndex` `ts_resolve.rs:587` (oxc_resolver on disk, kept only inside the corpus) | `resolved_import` kind `module` per specifier |
| python | `PyModuleIndex` `python/_2_modules.rs:249` | `resolved_import` kind `module` |
| kotlin | `KtModuleIndex` `kotlin_modules.rs:124` | `resolved_import` kind `module` + `star` |
| go | `GoModuleIndex` `go_modules.rs:204` | `resolved_import` target is the package DIR |
| rust | `RustModuleIndex` `rust_modules.rs` | `use` bindings only; `mod x;` edges live in `module_paths`, never emitted |

All five are built in `resolve_project_inputs` `project.rs:410-466` from a file list supplied up front. Measured on soopy (29 files, `ryi fast --sqlite`, 0.73s): a recursive CTE over `resolved_import` from `lib.rs` reaches 19 files; the other 10 hang off `mod` edges.

## 2. Target surface

### 2.0 Fast and slow: one projection, two producers

Role split (user-set 2026-09-25): SCIP and the checkers are the approving oracle. fast is what runs at the scale of hundreds of repos. Both write the SAME projection tables, so conformance is a SQL diff and fast is ratcheted toward slow.

| | fast | slow |
| --- | --- | --- |
| producer | tree-sitter / syn / oxc + scm queries + module plane + syntax legs | SCIP index (find, else build, cached); a checker where compiled |
| inputs | `Inputs` | `Inputs` (the index covers the root; rows are kept for the expanded files) |
| legs | syntax only; never reads an index | oracle only; no name-match legs |
| `resolved_edge` | caller site span -> callee def span, origin `same_file` / `module_plane` / ... | occurrence span -> definition span, origin `scip` / `checker` |
| `unresolved` | dropped sites with reason | occurrences whose definition is outside the corpus: reason `external`; `local ...` symbols: reason `local` |
| `resolved_import`, `resolved_type_edge`, `symbol`, `occurrence` | from the syntax planes | projected from SCIP occurrences / relationships |
| cost | ~0.7s for 29 files | indexer run per root (soopy: 51s) |

Grade = `slow.resolved_edge` vs `fast.resolved_edge` keyed on (path, site end, callee path, callee start): tp / wrong_target / overbound / miss.

Where the projection lives today:
- `tests/fixtures/ratchet_soopy/regen.sh`: SQL over `scip_occurrence` rows builds the oracle (definition by symbol, `local` / `corpus` / `external` class).
- `tests/170_ratchet_sites_rust.rs` `GRADE`: joins fast's `site` / `resolved_edge` / `unresolved` to that oracle on (path, occurrence end == site span end).
Both move into slow as its row producer, so any corpus gets an oracle by running `ryi slow`, and the ratchet reads two sqlite files.

What is mixed today and gets separated:
- `ScipMode::Off` with a root adopts a fresh cached index (`project.rs:1623`). `ryi fast` passes no root (`diet_scip_request`, `project.rs:1353-1354`), so fast is syntax-only today; the adoption reaches `--resolve --project-root R` and `graph --project-root R`.
- `ryi --resolve --scip-index I` runs the syntax legs AND the scip leg into one table (soopy: same_file 443 + scip 135 + ...). Its rows are neither pure fast nor pure oracle. The scip leg inside `Resolve<CallF>` is removed or kept behind a flag (open question).
- Today's `ryi slow ROOT` dumps raw `scip_*` rows that join to nothing fast writes; that dump moves to `ryi scip`.

Gaps for slow:
- one index per language present, merged (`scip_source_for` `project.rs:1704` refuses more than one language; `scip_decode.rs` already merges).
- python indexer (`ScipPython` exists in `scip.rs`, absent from `scip_source_for`).
- the checkers are compiled only with `--features rust-checker,ts-checker,go-checker`; slow names a missing one on stderr.

### 2.1 Semantic diff

| before | after | behavior change |
| --- | --- | --- |
| `ryi fast PATH...` / `--family diet_scip` | `ryi fast INPUTS` | dirs/globs/stdin/`--entry` |
| `ryi --resolve --family call,type [--scip-index I\|--scip-build] --project-root R PATH...` | `ryi slow INPUTS [--scip-index I]` | index found or built automatically; checkers included when compiled |
| `ryi slow ROOT` / `--family scip ROOT` (raw `scip_*` rows) | `ryi slow INPUTS` writes fast's tables from SCIP; raw rows via `ryi scip [--root DIR] [--index I] [--records K]` | slow output joins fast output |
| `--scip-facts --scip-record K` | `ryi scip --records K` | none |
| `ryi PATH` / `--family cst,call PATH` | `ryi fast --kinds cst,call INPUTS` without resolve, or kept as root default | open question 8 |
| `--rust-checker --ts-checker --go-checker` | gone from fast; slow uses every compiled checker as a second oracle; `--no-checker` to skip | a checker not compiled in is named in stderr once |
| `--deps` / `--scip-deps` / `--package-deps` | `resolved_import` already in both tiers; `file_edge` becomes a sqlite view over it; `package_edge` rides both tiers | none |
| `ryi --ingest F` / `--schema` / `--trail N` | `ryi ingest F` / `ryi schema` / `ryi trail [N]` | none |
| `--project-root DIR` / positional ROOT / `--root DIR` | `--root DIR` everywhere | default: git root of the first input, else cwd |
| `graph PATHS` | `graph INPUTS [--slow]` | graph reads the fast tables by default, slow tables with `--slow` |
| `graph --state DIR` | `graph --sqlite PATH` | same store, the path names the file |
| `watch --state P` | `watch --receipts P` | none |
| `graph --json` | removed; summary goes to stderr only when stderr is a tty | none on stdout |
| `diff --json` | removed | none |
| `watch --family` / `diff --family` | `--kinds` / `--arms` | none |
| `query --lang L FILE` | `query [--lang L] --query Q INPUTS` | `--lang` defaults from `RyiLang::from_path` per file; rows gain `path`; `--sqlite` lands `capture` rows |
| default log level `info` | `warn` | stderr quiet unless `RUST_LOG` |

`--pattern` keeps its current meaning (soopy glob) and moves into `Inputs`, so every file-taking verb has it.

### 2.2 Types (`src/bin/ryi/0_cli.rs`)

```rust
/// Every file-taking verb flattens this.
#[derive(Args)]
pub struct Inputs {
    /// Files, directories, or globs; - reads a path list from stdin
    #[arg(value_name = "PATH")]
    pub paths: Vec<String>,

    /// Keep files matching GLOB; repeatable
    #[arg(long = "pattern", value_name = "GLOB")]
    pub patterns: Vec<String>,

    /// Add every file FILE reaches over resolved imports
    #[arg(long, value_name = "FILE")]
    pub entry: Vec<PathBuf>,

    /// Import hops from --entry
    #[arg(long, value_name = "N", requires = "entry")]
    pub depth: Option<u32>,

    /// Corpus root (default: git root of the first input)
    #[arg(long, value_name = "DIR")]
    pub root: Option<PathBuf>,
}

/// fast and slow share every flag.
#[derive(Args)]
pub struct TierArgs {
    #[command(flatten)]
    pub inputs: Inputs,

    /// Write to a new SQLite database instead of stdout
    #[arg(long, value_name = "PATH")]
    pub sqlite: Option<PathBuf>,

    /// Add 1-based line/col beside every span
    #[arg(long)]
    pub lines: bool,

    /// Resolve arms (default call,type)
    #[arg(long, value_delimiter = ',', value_name = "ARMS")]
    pub arms: Vec<Arm>,

    /// Wrap output in the TSI envelope
    #[arg(long)]
    pub witness: bool,
}

#[derive(Args)]
pub struct SlowArgs {
    #[command(flatten)]
    pub tier: TierArgs,

    /// Load this index.scip instead of finding or building one
    #[arg(long, value_name = "FILE")]
    pub scip_index: Option<PathBuf>,

    /// Skip the compiler checkers
    #[arg(long)]
    pub no_checker: bool,

    /// Seconds allowed for one indexer run
    #[arg(long, value_name = "SECS")]
    pub scip_timeout: Option<u64>,
}

#[derive(Subcommand)]
pub enum Cmd {
    Fast(TierArgs),
    Slow(SlowArgs),
    Graph(GraphArgs),     // Inputs + arm group + --slow + --sqlite + --at/--compare
    Query(QueryArgs),     // Inputs + --lang? + --query + --digest + --sqlite
    Scip(ScipArgs),       // raw index rows: --root + --index + --records + --occurrence-text + --lines
    Cleave(CleaveArgs), Move(MoveArgs), Rename(RenameArgs), Region(RegionArgs),
    Watch(WatchArgs), Diff(DiffArgs),
    Ingest(IngestArgs), Schema, Trail(TrailArgs),
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Arm { Call, Type, Flow }
```

Library side, two producers of one row set:

```rust
/// Syntax only. Never loads an index.
pub fn fast_project(files: &[PathBuf], arms: ResolveArms) -> Result<Vec<FlatFact>, ProjectError> {
    // read_inputs_with_modules(files) -> resolve_project_inputs(scip: Off, no adoption) + scm_rows
}

/// Oracle only. SCIP occurrences (and checker answers when compiled) projected
/// onto resolved_edge / unresolved / resolved_import / resolved_type_edge / symbol / occurrence.
pub fn slow_project(files: &[PathBuf], root: &Path, index: Option<&Path>, checkers: bool)
    -> Result<Vec<FlatFact>, ProjectError> {
    // index = index or ensure_index_for_set(root, per language present) merged
    // for each occurrence in files: definition in corpus -> resolved_edge origin scip
    //                               definition outside -> unresolved external; local -> unresolved local
    // relationships is_implementation -> resolved_type_edge; import occurrences -> resolved_import
    // checkers -> same tables, origin checker
}
```

`diet_scip`/`diet_scip_request` become `fast_project`; `stream_scip_family`/`scip_family` become `ryi scip`; the regen.sh + `GRADE` SQL becomes `slow_project`.

`Ryi` loses `file: FileArgs`; `FileArgs`, `From<FastArgs>`, `From<SlowArgs>`, `FamilyMode`, `family_mode` go.

### 2.3 Expansion (`src/bin/ryi/1_inputs.rs`)

```rust
pub struct Expanded {
    pub root: PathBuf,          // canonical
    pub files: Vec<PathBuf>,    // root-relative, sorted, deduped
}

pub fn expand(inputs: &Inputs) -> Result<Expanded, String> {
    // root = inputs.root, else soopy::discover(first path or cwd).root, else cwd
    // tokens = inputs.paths, with "-" replaced by stdin lines (read once)
    // for token:
    //   file     -> push
    //   dir      -> soopy enumerate under dir with inputs.patterns (default: watch::default_patterns)
    //   else     -> treat as glob; soopy enumerate with [token]; zero matches -> Err("PATH matched nothing")
    // no tokens and no entry -> Err("no inputs")
    // keep files where source_for(path).is_some()
    // entry non-empty -> files = reach(root universe, entry, depth)  (2.4)
    // sort, dedup by canonical path
}
```

soopy supplies both walks: `SourceTree::enumerate` (worktree, gitignore on, `globset`) inside a repo, `DirectoryRoot::snapshot(FileQuery)` outside one. No new crate.

### 2.4 Entry crawl

```rust
pub fn reach(root: &Path, entry: &[PathBuf], depth: Option<u32>) -> Result<Vec<PathBuf>, String> {
    // universe = soopy enumerate root with default patterns
    // inputs = read_inputs_with_modules(universe)          (one batched read, one parse per file)
    // edges = import_facts over the five module indexes     (resolved_import rows)
    // BFS from entry over (src_path -> target_path), stop at depth or 32
    // go rows name a DIR: expand to that dir's .go files in the universe
}
```

Prerequisite: `RustModuleIndex::bindings` emits one `resolved_import` row of kind `module` per `mod x;` whose target is in the universe (the `module_paths` map at `rust_modules.rs:~429` already holds the pairs). The kind already exists (`ResolvedImportKind::Module`, python and kotlin emit it).

The crawl reads the whole root once. A lazy crawl would need every index to answer one file at a time; only `TsResolver` (disk-based) can today.

### 2.5 Checker selection (slow only)

```rust
fn checkers(no_checker: bool, files: &[PathBuf], root: &Path) -> (Option<&Path>, Option<&Path>, Option<&Path>) {
    // no_checker -> all None
    // languages = files.map(source_for(path).name()).dedup()
    // rust present && cfg!(feature = "rust-checker") -> Some(root); present but not compiled -> one stderr line naming the feature
    // same for ts (ts-checker), go (go-checker)
}
```

Fills the three existing `ResolveRequest` checker fields; `ResolveRequest` is unchanged.

## 3. Lifetimes

| value | born | dies |
| --- | --- | --- |
| `Ryi` / `Cmd` | `Ryi::try_parse` in `run` | end of `run` |
| stdin path list | first `-` token in `expand` | end of `expand` |
| soopy `SourceTree` / `DirectoryRoot` | once per `expand` | end of `expand` |
| `Expanded` | `expand` | handed by value to the verb |
| module indexes for `--entry` | `reach` | end of `reach`; the verb re-reads its own set |
| `sqlite::Database` | verb start | `finish`/`close` |

No state persists between runs beyond what `--sqlite`, `--receipts`, soopy `--state` and the run trail already persist.

## 4. Storage, reads, writes

- stdin: read at most once, only when a `-` token is present.
- filesystem: one soopy walk per directory or glob token; one read per file (existing `read_inputs_batched`); `--entry` adds one read of the root universe.
- writes: unchanged sinks (stdout JSONL, `--sqlite` staging + `persist_noclobber`).
- uniqueness: a file is its canonical root-relative path; `./a.ts` and `a.ts` are one input; the list is sorted so output order does not depend on spelling.

## 5. Deletions

`check_file_paths` directory refusal (`ryi.rs:225`), `expand_paths` (`0_graph.rs:292`), `expand`/`walk` (`7_scm_rows.rs:319-348`, `scm_facts` takes the expanded list), `FileArgs` and both `From` impls, `family_mode`, `FamilyMode`, the three checker bools on the clap structs, `schema/3_cli.tsp` (eyeball-only model of the old surface, not built).

## 6. Test migration

Test files under `tests/` that pass each old flag as a string literal (`rg -l -F '"--flag"'`): `--resolve` 89, `--family` 83, `--project-root` 30, `--witness` 20, `--state` 18, `--rust-checker` 11, `--ingest` 9, `--scip-facts` 8, `--scip-build` 8, `--schema` 6, `--bench` 5, `--file-fact` 5, `--ts-checker` 5, `--scip-deps` 4, `--deps` 3, `--package-deps` 2, `--max-bytes` 2, `--go-checker` 1, `--trail` 1.

## 7. Order

1. `Inputs` + `expand` on `fast`, `graph`, `query`; delete the walkers. Ratchet 170 stays green.
2. `slow_project` from the regen.sh/`GRADE` SQL; fast stops adopting cached indexes; old `slow` moves to `ryi scip`; ratchet 170 diffs two sqlite files. Multi-language index merge.
3. Rust `mod` rows in `resolved_import`; `--entry`/`--depth`.
4. Root flags to verbs (`ingest`, `schema`, `trail`); `--arms`/`--kinds`; `--root`; `--state`/`--json` splits; test migration.
5. Default log `warn`; `query` path column and `--sqlite`.

## 7c. Decisions 2026-09-25 (user-set), queued as the post-M6 CLI milestone

- graph keeps and reuses its own store per invocation: `<root>/.dl/.state/graph-<key>.db`, stamped with `corpus(ryi_build, tier, arms, root, head, status_hash)` + `file(path, digest)`. Each run: open, compare HEAD + `git status --porcelain=v2` hash, re-hash only listed paths, report changed/moved/added/removed, re-extract when stale, answer, exit. `--db PATH` points at a store built by `fast`/`slow --sqlite`. No daemon. crates/sprefa-extract/AGENTS.md states this as the current rule.
- Logging: console printer default `warn`; summary/trail layer keeps its own `debug` filter and writes per-phase stats to ~/.agent/dl6.db every run (`ryi trail` reads them); per-file detail to disk on request (`HAFLEY_TRACE`, `HAFLEY_LOG_SQLITE`). Replaces the 2026-09-18 `sprefa_extract=info` console default (trace.rs:584); update tests/31_tracing.rs.
- Every fork report ends with a ryi usability survey (discoverability, plan readability, stop messages, speed, trust, top time-saver).

## 7b. Refactor rule (user-set 2026-09-25)

Every code relocation (M6 read side into hafley_scm, later TS/Kotlin moves into the new layout) runs through `ryi move` / `ryi cleave` only. A gap in the tool is filed as an issue and fixed in the tool on the spot, then the move is re-run. No hand edits of `use`/import paths, `mod` decls or manifests.

## 7a. Deprioritized

- `ryi region` (DL7 marker regions, `3_region_writer.rs`, `lang/4_owned_region.rs`): kept as is; DL7 is now dl8, same marker format. No work planned (user-set 2026-09-25).

## 8. Open questions

- The scip leg inside `Resolve<CallF>` (`--resolve --scip-index`, mixed table): remove, or keep as `fast --with-scip` for single-repo use?
- Hard cut, or keep the old root flags as hidden aliases for one release? Recommendation: hard cut; the test migration is mechanical.
- `slow` positional ROOT: fold into `--root`, or keep positional?
- Per-file phase-1 extraction (`ryi PATH`, `--family cst,...`, `--bench`, `--file-fact`, `--max-bytes`): keep as the bare root command, or fold into `fast --kinds` with no resolve?
- The five module indexes live in sprefa-extract (`ts_resolve.rs`, `rust_modules.rs`, `go_modules.rs`, `python/_2_modules.rs`, `kotlin_modules.rs`). Moving them into hafley_scm is its own issue.
