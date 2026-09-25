// @comment-ok: eyeball-only model of ryi's clap surface (origin/feat/graph-views);
// not wired into the crate build. Root model named RyiCli per task instructions;
// the emitter appends "Cli" to a command model's own name, so the generated root
// struct is RyiCliCli, not RyiCli -- see final report for the naming collision.
//
// @comment-ok: the Cli namespace (packages/binding-core/lib/cli.tsp) exposes only
// command, subcommand, cliFlag, cliArg(position), short(char) and about(text).
// Every other clap attribute on the real structs is dropped by this model, not
// just the ones called out struct-by-struct below:
//   - #[command(name = "...")] on every subcommand: real program names mix "ryi"
//     (root, cleave, cleave's own subcommand block reads "ryi cleave", graph
//     reads "ryi graph") and "extract" (move, rename, query, region read
//     "extract move" etc, the pre-rename binary name); the emitter always
//     derives the clap name from the model name instead.
//   - root Cli: `version`, `long_about = LONG_ABOUT` (about wins, long_about
//     dropped), `after_help = AFTER_HELP`.
//   - root Cli.paths: `required_unless_present_any = ["schema","ingest","trail"]`,
//     `long_help = PATH_LONG` (about wins over long_help everywhere it differs).
//   - root Cli.family: `value_delimiter = ','`.
//   - almost every root/Fast/Slow flag carries `conflicts_with_all`/`requires`/
//     `requires_all` (e.g. sqlite conflicts with bench/schema/trail; resolve
//     conflicts with the ast-* flags; rust_checker/ts_checker/go_checker/
//     scip_build require project_root; scip_record/occurrence_text/ast_selector/
//     ast_capture require scip_facts/ast_pattern respectively) -- none of it is
//     expressible, so the generated structs accept combinations the real CLI
//     rejects.
//   - ingest: `num_args = 1..` (repeatable positional-like flag) plus its own
//     conflicts_with_all list.
//   - ast_pattern/ast_selector/ast_capture: `action = clap::ArgAction::Append`.
//   - trail: `num_args = 0..=1`, `default_missing_value = "5"`.
//   - rust_checker/ts_checker/go_checker/scip_record/occurrence_text/
//     package_deps/max_bytes/ast_pattern/ast_selector/ast_capture/verify (on
//     cleave/move)/text_refs (on cleave/move/rename)/verify_cwd/relocate_mod/
//     verify_scip: `long = "kebab-case-name"` overriding the default
//     snake_case-to-kebab conversion clap would otherwise do for a
//     multi-word field -- the emitter always kebab-cases the field name, so
//     these happen to already match and the override is silently redundant,
//     but it is still a hand-written attribute with no decorator equivalent.
//   - Graph and Cleave (not just Rename, see below) both carry
//     `after_help = SCOPE`, an out-of-scope/design-notes paragraph; only
//     Rename's exit-code legend is called out locally.
//   - Graph: `#[command(group(ArgGroup::new("arm").required(true)
//     .args(["callers","uses","from"])))]`.
//   - Region.generated's `default_value = "-"` and Watch.poll_ms's `500`
//     round-trip correctly via @Entity.default (the only default decorator
//     that exists in this toolchain; the task brief's @Config.default does
//     not exist -- Config only has source/envVar/secret/path).
//   - array-typed fields (family, ingest, ast_pattern, ast_selector,
//     ast_capture, root on move, pattern on watch/diff) emit as a single
//     Rust `String`/`Option<String>` and #[arg(long)] with no repeat action,
//     not `Vec<...>`: an emitter limitation on `T[]`, not a decorator gap,
//     but the generated struct behaves like an ArgGroup of one and needs
//     hand-fixing before it could replace the real one.

import "../../../../../hafley-tsp/packages/binding-core/lib/main.tsp";

using Cli;
using Entity;

@Cli.command
@Cli.about("sprefa-extract: one source file -> flat graph facts (JSONL to stdout)")
model RyiCli {
  @Cli.cliArg(0)
  @Cli.about("A source file to extract; output goes to stdout unless --bench")
  paths: string[];

  @Cli.cliFlag
  @Cli.about("Which kinds of facts to extract, comma-separated: cst, type, call, df, data, cfg")
  family: string[] | null;

  @Cli.cliFlag
  @Cli.about("Write facts to a NEW SQLite database at PATH, then print schema/query commands")
  sqlite: string | null;

  @Cli.cliFlag
  @Cli.about("Time extract + flatten and report per-family counts to stderr")
  bench: boolean;

  @Cli.cliFlag
  @Cli.about("Resolve cross-file edges across all supplied paths (see --family)")
  resolve: boolean;

  @Cli.cliFlag
  @Cli.about("Root that SCIP document paths are relative to; also the --scip-build root")
  project_root: string | null;

  @Cli.cliFlag
  @Cli.about("Load a prebuilt index.scip into the resolve context")
  scip_index: string | null;

  @Cli.cliFlag
  @Cli.about("Answer rust call/type destinations with rust-analyzer's own resolution")
  rust_checker: boolean;

  @Cli.cliFlag
  @Cli.about("Answer ts call/type destinations with the TypeScript compiler's own resolution")
  ts_checker: boolean;

  @Cli.cliFlag
  @Cli.about("Answer go call/type destinations with go/types' own resolution")
  go_checker: boolean;

  @Cli.cliFlag
  @Cli.about("Build the index with the language's own indexer, then load it")
  scip_build: boolean;

  @Cli.cliFlag
  @Cli.about("Stream file-to-file dependency edges folded from a SCIP index")
  scip_deps: boolean;

  @Cli.cliFlag
  @Cli.about("Stream the whole SCIP index as facts, every field the protobuf carries")
  scip_facts: boolean;

  @Cli.cliFlag
  @Cli.about("Narrow --scip-facts to a comma-separated list of record kinds")
  scip_record: string | null;

  @Cli.cliFlag
  @Cli.about("Also carry the source slice at each scip_occurrence span, as text")
  occurrence_text: boolean;

  @Cli.cliFlag
  @Cli.about("Stream file_edge rows resolved syntactically, with no SCIP index")
  deps: boolean;

  @Cli.cliFlag
  @Cli.about("Stream package_edge rows: workspace-internal manifest-to-manifest edges")
  package_deps: boolean;

  @Cli.cliFlag
  @Cli.about("Prepend one file record: path, content digest, byte count, line count")
  file_fact: boolean;

  @Cli.cliFlag
  @Cli.about("Decorate stdout: 1-based line and col beside every start/end span")
  lines: boolean;

  @Cli.cliFlag
  @Cli.about("Wrap the stream in the TSI envelope")
  witness: boolean;

  @Cli.cliFlag
  @Cli.about("Read foreign TSI JSONL, validate it against the relation registry, and re-emit it")
  ingest: string[];

  @Cli.cliFlag
  @Cli.about("Byte ceiling for one input; over it emits size_skip and exits 0 (0 = none)")
  max_bytes: uint64 | null;

  @Cli.cliFlag
  @Cli.about("Ast-grep pattern in ID=PATTERN form; repeat to batch patterns over one parse")
  ast_pattern: string[];

  @Cli.cliFlag
  @Cli.about("Contextual pattern selector in ID=KIND form; repeat at most once per query")
  ast_selector: string[];

  @Cli.cliFlag
  @Cli.about("Single-node metavariable to emit in ID=NAME form; repeat per query")
  ast_capture: string[];

  @Cli.cliFlag
  @Cli.about("Where --family scip places and finds its index cache")
  scip_cache: string | null;

  @Cli.cliFlag
  @Cli.about("Wall budget in seconds for ONE indexer run under --family scip")
  scip_timeout: uint64 | null;

  @Cli.cliFlag
  @Cli.about("Run ONE named SCIP indexer under --family scip instead of every roster match")
  indexer: string | null;

  @Cli.cliFlag
  @Cli.about("Print the JSONL output contract to stdout and exit (no extraction)")
  schema: boolean;

  @Cli.cliFlag
  @Cli.about("Print the last N runs of the on-disk trail and exit (no extraction)")
  trail: uint64 | null;
}

// @comment-ok: ryi fast pins --family diet_scip via alias_args() in ryi.rs and
// rejects --family/--scip-index/--scip-build/--rust-checker/--ts-checker/
// --go-checker/--indexer; every other root flag still applies, duplicated here
// because the decorator set has no field-inheritance into the CLI zone.
@Cli.command
@Cli.subcommand(RyiCli)
@Cli.about("alias: pins --family diet_scip, then parses the same flags as the root command")
model Fast {
  @Cli.cliArg(0)
  @Cli.about("A source file to extract; output goes to stdout unless --bench")
  paths: string[];

  @Cli.cliFlag
  @Cli.about("Write facts to a NEW SQLite database at PATH, then print schema/query commands")
  sqlite: string | null;

  @Cli.cliFlag
  @Cli.about("Time extract + flatten and report per-family counts to stderr")
  bench: boolean;

  @Cli.cliFlag
  @Cli.about("Resolve cross-file edges across all supplied paths (see --family)")
  resolve: boolean;

  @Cli.cliFlag
  @Cli.about("Root that SCIP document paths are relative to; also the --scip-build root")
  project_root: string | null;

  @Cli.cliFlag
  @Cli.about("Build the index with the language's own indexer, then load it")
  scip_build: boolean;

  @Cli.cliFlag
  @Cli.about("Stream file-to-file dependency edges folded from a SCIP index")
  scip_deps: boolean;

  @Cli.cliFlag
  @Cli.about("Stream the whole SCIP index as facts, every field the protobuf carries")
  scip_facts: boolean;

  @Cli.cliFlag
  @Cli.about("Narrow --scip-facts to a comma-separated list of record kinds")
  scip_record: string | null;

  @Cli.cliFlag
  @Cli.about("Also carry the source slice at each scip_occurrence span, as text")
  occurrence_text: boolean;

  @Cli.cliFlag
  @Cli.about("Stream file_edge rows resolved syntactically, with no SCIP index")
  deps: boolean;

  @Cli.cliFlag
  @Cli.about("Stream package_edge rows: workspace-internal manifest-to-manifest edges")
  package_deps: boolean;

  @Cli.cliFlag
  @Cli.about("Prepend one file record: path, content digest, byte count, line count")
  file_fact: boolean;

  @Cli.cliFlag
  @Cli.about("Decorate stdout: 1-based line and col beside every start/end span")
  lines: boolean;

  @Cli.cliFlag
  @Cli.about("Wrap the stream in the TSI envelope")
  witness: boolean;

  @Cli.cliFlag
  @Cli.about("Read foreign TSI JSONL, validate it against the relation registry, and re-emit it")
  ingest: string[];

  @Cli.cliFlag
  @Cli.about("Byte ceiling for one input; over it emits size_skip and exits 0 (0 = none)")
  max_bytes: uint64 | null;

  @Cli.cliFlag
  @Cli.about("Ast-grep pattern in ID=PATTERN form; repeat to batch patterns over one parse")
  ast_pattern: string[];

  @Cli.cliFlag
  @Cli.about("Contextual pattern selector in ID=KIND form; repeat at most once per query")
  ast_selector: string[];

  @Cli.cliFlag
  @Cli.about("Single-node metavariable to emit in ID=NAME form; repeat per query")
  ast_capture: string[];

  @Cli.cliFlag
  @Cli.about("Where --family scip places and finds its index cache")
  scip_cache: string | null;

  @Cli.cliFlag
  @Cli.about("Wall budget in seconds for ONE indexer run under --family scip")
  scip_timeout: uint64 | null;

  @Cli.cliFlag
  @Cli.about("Print the JSONL output contract to stdout and exit (no extraction)")
  schema: boolean;

  @Cli.cliFlag
  @Cli.about("Print the last N runs of the on-disk trail and exit (no extraction)")
  trail: uint64 | null;
}

// @comment-ok: ryi slow pins --family scip the same way as fast, but --indexer
// is still allowed (only the six MODE_FLAGS are forbidden), so this is Fast
// plus indexer.
@Cli.command
@Cli.subcommand(RyiCli)
@Cli.about("alias: pins --family scip, then parses the same flags as the root command")
model Slow {
  @Cli.cliArg(0)
  @Cli.about("A source file to extract; output goes to stdout unless --bench")
  paths: string[];

  @Cli.cliFlag
  @Cli.about("Write facts to a NEW SQLite database at PATH, then print schema/query commands")
  sqlite: string | null;

  @Cli.cliFlag
  @Cli.about("Time extract + flatten and report per-family counts to stderr")
  bench: boolean;

  @Cli.cliFlag
  @Cli.about("Resolve cross-file edges across all supplied paths (see --family)")
  resolve: boolean;

  @Cli.cliFlag
  @Cli.about("Root that SCIP document paths are relative to; also the --scip-build root")
  project_root: string | null;

  @Cli.cliFlag
  @Cli.about("Build the index with the language's own indexer, then load it")
  scip_build: boolean;

  @Cli.cliFlag
  @Cli.about("Stream file-to-file dependency edges folded from a SCIP index")
  scip_deps: boolean;

  @Cli.cliFlag
  @Cli.about("Stream the whole SCIP index as facts, every field the protobuf carries")
  scip_facts: boolean;

  @Cli.cliFlag
  @Cli.about("Narrow --scip-facts to a comma-separated list of record kinds")
  scip_record: string | null;

  @Cli.cliFlag
  @Cli.about("Also carry the source slice at each scip_occurrence span, as text")
  occurrence_text: boolean;

  @Cli.cliFlag
  @Cli.about("Stream file_edge rows resolved syntactically, with no SCIP index")
  deps: boolean;

  @Cli.cliFlag
  @Cli.about("Stream package_edge rows: workspace-internal manifest-to-manifest edges")
  package_deps: boolean;

  @Cli.cliFlag
  @Cli.about("Prepend one file record: path, content digest, byte count, line count")
  file_fact: boolean;

  @Cli.cliFlag
  @Cli.about("Decorate stdout: 1-based line and col beside every start/end span")
  lines: boolean;

  @Cli.cliFlag
  @Cli.about("Wrap the stream in the TSI envelope")
  witness: boolean;

  @Cli.cliFlag
  @Cli.about("Read foreign TSI JSONL, validate it against the relation registry, and re-emit it")
  ingest: string[];

  @Cli.cliFlag
  @Cli.about("Byte ceiling for one input; over it emits size_skip and exits 0 (0 = none)")
  max_bytes: uint64 | null;

  @Cli.cliFlag
  @Cli.about("Ast-grep pattern in ID=PATTERN form; repeat to batch patterns over one parse")
  ast_pattern: string[];

  @Cli.cliFlag
  @Cli.about("Contextual pattern selector in ID=KIND form; repeat at most once per query")
  ast_selector: string[];

  @Cli.cliFlag
  @Cli.about("Single-node metavariable to emit in ID=NAME form; repeat per query")
  ast_capture: string[];

  @Cli.cliFlag
  @Cli.about("Where --family scip places and finds its index cache")
  scip_cache: string | null;

  @Cli.cliFlag
  @Cli.about("Wall budget in seconds for ONE indexer run under --family scip")
  scip_timeout: uint64 | null;

  @Cli.cliFlag
  @Cli.about("Run ONE named SCIP indexer under --family scip instead of every roster match")
  indexer: string | null;

  @Cli.cliFlag
  @Cli.about("Print the JSONL output contract to stdout and exit (no extraction)")
  schema: boolean;

  @Cli.cliFlag
  @Cli.about("Print the last N runs of the on-disk trail and exit (no extraction)")
  trail: uint64 | null;
}

// @comment-ok: real command also carries #[command(group(ArgGroup::new("arm")
// .required(true).args(["callers","uses","from"])))], not expressible here.
@Cli.command
@Cli.subcommand(RyiCli)
@Cli.about("ask one question of the resolved call and type graph of a corpus")
model Graph {
  @Cli.cliArg(0)
  @Cli.about("Files and directories; a directory is walked by the language roster")
  paths: string[];

  @Cli.cliFlag
  @Cli.about("Who calls NAME: one row per resolved call edge landing on it")
  callers: string | null;

  @Cli.cliFlag
  @Cli.about("Who references the type NAME: one row per referencing declaration")
  uses: string | null;

  @Cli.cliFlag
  @Cli.about("What NAME reaches along resolved call edges, transitively")
  from: string | null;

  @Cli.cliFlag
  @Cli.about("Publish the fact store as DIR/graph.db instead of keeping it in memory")
  state: string | null;

  @Cli.cliFlag
  @Cli.about("Drop the stderr summary line; stdout is JSONL either way")
  json: boolean;
}

@Cli.command
@Cli.subcommand(RyiCli)
@Cli.about("move one item out of a file into another, with the imports it needs")
model Cleave {
  @Cli.cliArg(0)
  @Cli.about("<SRC>#<ITEM>: the file the item is declared in and its name")
  target: string;

  @Cli.cliArg(1)
  @Cli.about("The file it lands in; created when it does not exist")
  dest: string;

  @Cli.cliFlag
  @Cli.about("Corpus root; defaults to the git root holding SRC")
  root: string | null;

  @Cli.cliFlag
  @Cli.about("Soopy state root; must sit outside the corpus root")
  state: string | null;

  @Cli.cliFlag
  @Cli.about("Move the same-file private helpers only the item uses")
  drag: boolean;

  @Cli.cliFlag
  @Cli.about("Apply the plan to the real tree instead of dry running it")
  commit: boolean;

  @Cli.cliFlag
  @Cli.about("Run this shell command in the root after --commit; rolls back on failure")
  verify: string | null;

  @Cli.cliFlag
  @Cli.about("Report the SRC spellings this cleave leaves behind in plain text")
  text_refs: boolean;

  @Cli.cliFlag
  @Cli.about("Close the output with one JSON line carrying the whole plan")
  json: boolean;
}

@Cli.command
@Cli.subcommand(RyiCli)
@Cli.about("move a file and repair every specifier that named it")
model Move {
  @Cli.cliArg(0)
  @Cli.about("The file to rehome; omitted when --list carries the moves")
  old: string | null;

  @Cli.cliArg(1)
  @Cli.about("Where it lands")
  new: string | null;

  @Cli.cliFlag
  @Cli.about("A tsv of old<TAB>new rows, one move per line")
  list: string | null;

  @Cli.cliFlag
  @Cli.about("Corpus root; repeatable, each root gets its own plan and stage batch")
  root: string[];

  @Cli.cliFlag
  @Cli.about("Directory the --verify command runs in; defaults to the first root")
  verify_cwd: string | null;

  @Cli.cliFlag
  @Cli.about("Soopy state root; must sit outside the corpus root")
  state: string | null;

  @Cli.cliFlag
  @Cli.about("Apply the plan to the real tree instead of dry running it")
  commit: boolean;

  @Cli.cliFlag
  @Cli.about("Leave a reexport shim behind at old instead of rewriting importers")
  shim: boolean;

  @Cli.cliFlag
  @Cli.about("Relocate a moved Rust module's mod declaration instead of adding #[path]")
  relocate_mod: boolean;

  @Cli.cliFlag
  @Cli.about("Run this shell command in the move root after --commit; rolls back on failure")
  verify: string | null;

  @Cli.cliFlag
  @Cli.about("Report the old-path spellings this move leaves behind in plain text")
  text_refs: boolean;
}

// @comment-ok: real command also carries after_help with the exit-code legend
// (2 plan error, 3 ambiguous, 4 not found, 5 inexact, 6 dynamic, 7 abstains),
// not expressible as @about (single string, no separate long-form slot).
@Cli.command
@Cli.subcommand(RyiCli)
@Cli.about("rename a symbol and respell every occurrence bound to it")
model Rename {
  @Cli.cliArg(0)
  @Cli.about("<FILE>#<OLD>: the declaring file and the identifier as written today")
  target: string | null;

  @Cli.cliArg(1)
  @Cli.about("What the identifier becomes")
  new: string | null;

  @Cli.cliFlag
  @Cli.about("A tsv of anchor<TAB>old<TAB>new rows, one rename per line")
  list: string | null;

  @Cli.cliFlag
  @Cli.about("Corpus root; defaults to the git root holding the first anchor")
  root: string | null;

  @Cli.cliFlag
  @Cli.about("Soopy state root; must sit outside the corpus root")
  state: string | null;

  @Cli.cliFlag
  @Cli.about("Byte offset inside the declaration, when the anchor declares OLD more than once")
  at: uint32 | null;

  @Cli.cliFlag
  @Cli.about("Apply the plan to the real tree instead of dry running it")
  commit: boolean;

  @Cli.cliFlag
  @Cli.about("Report the old-name spellings this rename leaves behind in plain text")
  text_refs: boolean;

  @Cli.cliFlag
  @Cli.about("Cross-check the plan against a prebuilt SCIP index; report only")
  verify_scip: string | null;

  @Cli.cliFlag
  @Cli.about("Close the output with one JSON line naming the sites the arm declined to plan")
  json: boolean;
}

// @comment-ok: real struct carries no about/help text anywhere (name-only
// #[command], bare #[arg(long)]), so none is modelled here either.
@Cli.command
@Cli.subcommand(RyiCli)
model Query {
  @Cli.cliFlag
  lang: string;

  @Cli.cliFlag
  query: string;

  @Cli.cliFlag
  digest: string | null;

  @Cli.cliArg(0)
  path: string;
}

// @comment-ok: not a clap struct in the real binary -- hand-rolled in
// 4_watch.rs (Options + fn parse), matched as a bare string against
// std::env::args().nth(1) in ryi.rs. Modelled from parse()'s match arms and
// its usage string.
@Cli.command
@Cli.subcommand(RyiCli)
@Cli.about("stream fact deltas for a working tree as it changes")
model Watch {
  @Cli.cliArg(0)
  @Cli.about("Corpus root to watch")
  root: string;

  @Cli.cliFlag
  @Cli.about("Glob to watch; repeatable, defaults to the built-in language globs")
  pattern: string[];

  @Cli.cliFlag
  @Cli.about("Comma-separated family names, same vocabulary as the root command")
  family: string[] | null;

  @Cli.cliFlag
  @Cli.about("Receipt-store state path")
  state: string | null;

  @Cli.cliFlag
  @Cli.about("Emit one snapshot and exit instead of watching")
  once: boolean;

  @Cli.cliFlag
  @Entity.default("500")
  @Cli.about("Poll interval in milliseconds when the platform watcher is unavailable")
  poll_ms: uint32;
}

// @comment-ok: not a clap struct -- hand-rolled in 5_diff.rs (Options + fn
// parse). Modelled from its usage string "usage: extract diff ROOT --from REV
// --to REV [--pattern GLOB] [--family call,type] [--sqlite PATH] [--json]".
@Cli.command
@Cli.subcommand(RyiCli)
@Cli.about("the fact delta between two commits, one shot")
model Diff {
  @Cli.cliArg(0)
  @Cli.about("Corpus root")
  root: string;

  @Cli.cliFlag
  @Cli.about("Revision to diff from (required)")
  from: string;

  @Cli.cliFlag
  @Cli.about("Revision to diff to (required)")
  to: string;

  @Cli.cliFlag
  @Cli.about("Glob to include; repeatable, defaults to the watch verb's language globs")
  pattern: string[];

  @Cli.cliFlag
  @Cli.about("Resolve arms to diff: call and/or type; defaults to both")
  family: string[] | null;

  @Cli.cliFlag
  @Cli.about("Write rows to a NEW SQLite database at PATH instead of stdout JSONL")
  sqlite: string | null;

  @Cli.cliFlag
  @Cli.about("No-op: JSONL is the only wire this verb writes")
  json: boolean;
}

// @comment-ok: real struct carries a name-only #[command] (no about), so none
// is modelled at the command level either.
@Cli.command
@Cli.subcommand(RyiCli)
model Region {
  @Cli.cliArg(0)
  @Cli.about("DL7 file containing the owned comment markers")
  target: string;

  @Cli.cliArg(1)
  @Cli.about("Marker identifier following sprefa:auto-begin and sprefa:auto-end")
  id: string;

  @Cli.cliFlag
  @Entity.default("-")
  @Cli.about("Generated body file, or - for stdin")
  generated: string;

  @Cli.cliFlag
  @Cli.about("Commit the content-guarded replacement; without it, report drift")
  apply: boolean;

  @Cli.cliFlag
  @Cli.about("Soopy state root used by an applied mutation")
  state: string | null;
}
