import "@typespec/http";
import "@typespec/http/streams";
import "./domain.tsp";
import "@hafley/typespec-decorator-def";

using Http;
using Http.Streams;
using Clap;

@service
@rootArgs(FileArgs)
@argsConflictsWithSubcommands
@doc("Source files -> flat graph facts (JSONL on stdout, or --sqlite)")
@afterHelp("Logging: RUST_LOG (default sprefa_extract=info,hafley_scm=info), HAFLEY_LOG_FORMAT=json|text\nBuild: git hash: $SPREFA_BUILD_GIT_HASH, datetime: $SPREFA_BUILD_DATETIME")
namespace Ryi;

/** Syntax-only whole-project facts (no compiler) */
@route("/fast")
@post
op fast(
  /** Files, directories, or globs; - reads a path list from stdin */
  @skip @query paths?: string[],

  @bodyRoot inputs: Inputs,

  /** Write to a new SQLite database instead of stdout */
  @valueName("PATH") @query sqlite?: path,

  /** Add 1-based line/col beside every span */
  @query lines?: boolean,
): JsonlStream<TypeEdge>;

/** The SCIP oracle written as fast's tables */
@route("/slow")
@post
op slow(
  /** Files, directories, or globs; - reads a path list from stdin */
  @skip @query paths?: string[],

  @bodyRoot inputs: Inputs,

  /** Write to a new SQLite database instead of stdout */
  @valueName("PATH") @query sqlite?: path,

  /** Add 1-based line/col beside every span */
  @query lines?: boolean,

  /** Load this index.scip instead of finding or building one */
  @valueName("FILE") @query scip_index?: path,

  /** Skip the compiler checkers */
  @query no_checker?: boolean,

  /** Seconds allowed for one indexer run */
  @valueName("SECS") @query scip_timeout?: uint64,
): FactSummary;

/** Raw SCIP index rows */
@route("/scip")
@post
op scip(
  /** Files, directories, or globs; - reads a path list from stdin */
  @skip @query paths?: string[],

  @bodyRoot inputs: Inputs,

  /** Write to a new SQLite database instead of stdout */
  @valueName("PATH") @query sqlite?: path,

  /** Add 1-based line/col beside every span */
  @query lines?: boolean,

  /** Load this index.scip instead of finding or building one */
  @valueName("FILE") @conflictsWith(#["indexer"]) @query scip_index?: path,

  /** SCIP index cache directory */
  @valueName("DIR") @query scip_cache?: path,

  /** Seconds allowed for one indexer run */
  @valueName("SECS") @query scip_timeout?: uint64,

  /** Run only this language's SCIP indexer */
  @valueName("LANG") @query indexer?: string,

  /** Stream the index records themselves instead of the scip_* relations */
  @query raw?: boolean,

  /** Only these --raw record kinds (comma-separated) */
  @valueName("KINDS") @requires("raw") @query records?: string,

  /** Add the source text to each scip_occurrence */
  @requires("raw") @query occurrence_text?: boolean,

  /** Build the index for --raw with the inputs' language indexer */
  @requiresAll(#["raw", "root"]) @conflictsWith(#["scip_index"]) @query scip_build?: boolean,
): FactSummary;

/** Ask one question of the resolved call/type graph */
@route("/graph")
@post
@requiredOneOf(#["callers", "uses", "from", "call_path", "type_path", "flow_path"], "arm")
op graph(
  /** Files, directories, or globs; - reads a path list from stdin */
  @skip @query paths?: string[],

  @bodyRoot inputs: Inputs,

  /** Resolved call edges landing on NAME */
  @valueName("NAME") @query callers?: string,

  /** Declarations that reference type NAME */
  @valueName("NAME") @query uses?: string,

  /** Everything NAME reaches along call edges */
  @valueName("NAME") @query from?: string,

  /** Shortest call paths from NAME */
  @valueName("NAME") @query call_path?: string,

  /** Shortest type-reference paths from NAME */
  @valueName("NAME") @query type_path?: string,

  @doc("Flow paths from BLOB@START:END")
  @valueName("BLOB@START:END") @query flow_path?: string,

  /** Keep the fact store in a new SQLite database at PATH */
  @valueName("PATH") @query sqlite?: path,

  /** Walk the SCIP oracle's edges (ryi slow) instead of the syntax resolve */
  @conflictsWith(#["rust_checker", "ts_checker", "go_checker"]) @query slow?: boolean,

  /** Seconds the question may run; past it graph exits 3 */
  @minValue(1) @valueName("SECS") @query timeout?: uint64 = 30,

  /** Query a committed revision */
  @valueName("REV") @requires("root") @conflictsWith(#["rust_checker", "ts_checker", "go_checker", "scip_index", "sqlite"]) @query at?: string,

  /** Diff the answers against REV */
  @valueName("REV") @requires("at") @conflictsWith(#["sqlite"]) @query compare?: string,

  /** Load this index.scip */
  @valueName("FILE") @requires("root") @query scip_index?: path,

  /** Add rust-analyzer type evidence */
  @requires("root") @query rust_checker?: boolean,

  /** Add TypeScript checker type evidence */
  @requires("root") @query ts_checker?: boolean,

  /** Add go/types type evidence */
  @requires("root") @query go_checker?: boolean,
): CallEdge[];

/** Move one item into another file, with its imports */
@route("/cleave")
@post
op cleave(
  /** SRC#ITEM (omit with --list) */
  @positional @query target?: string,

  /** Destination file, created if missing (omit with --list) */
  @positional @query dest?: path,

  /** TSV of SRC#ITEM<TAB>DEST rows, applied in order as one stage */
  @conflictsWith(#["target", "dest", "json"]) @query list?: path,

  /** Corpus root (default: git root of SRC) */
  @query root?: path,

  /** Soopy state root, outside the corpus */
  @query state?: path,

  /** Also move private helpers only this item uses */
  @query drag?: boolean,

  /** Apply instead of dry run */
  @query commit?: boolean,

  /** Command to run after --commit; failure rolls back */
  @query verify?: string,

  /** Report leftover SRC spellings in plain text */
  @query text_refs?: boolean,

  /** End with one JSON line holding the plan */
  @query json?: boolean,
): EditPlan;

/** Move a file and repair every specifier that names it */
@route("/move")
@post
op `move`(
  /** File to move (omit with --list) */
  @positional @query old?: path,

  /** Destination */
  @positional @query new?: path,

  /** TSV of old<TAB>new rows */
  @query list?: path,

  /** Corpus root; repeatable */
  @query root?: path[],

  /** Directory --verify runs in (default: first root) */
  @query verify_cwd?: path,

  /** Soopy state root, outside the corpus */
  @query state?: path,

  /** Apply instead of dry run */
  @query commit?: boolean,

  /** Leave a reexport shim at OLD instead of rewriting importers */
  @query shim?: boolean,

  /** Move a Rust module's `mod` line instead of adding #[path] */
  @query relocate_mod?: boolean,

  /** Command to run after --commit; failure rolls back */
  @query verify?: string,

  /** Report leftover old-path spellings in plain text */
  @query text_refs?: boolean,
): EditPlan;

/** Rename a symbol and every occurrence bound to it */
@route("/rename")
@post
@afterHelp("Exit codes: 2 plan error, 3 ambiguous (pass --at), 4 not found, 5 inexact, 6 dynamic, 7 plan has abstains")
op rename(
  /** FILE#OLD (omit with --list) */
  @positional @query target?: string,

  /** New name */
  @positional @query new?: string,

  /** TSV of anchor<TAB>old<TAB>new rows */
  @query list?: path,

  /** Corpus root (default: git root of the first anchor) */
  @query root?: path,

  /** Soopy state root, outside the corpus */
  @query state?: path,

  /** Byte offset of the declaration when OLD is declared twice */
  @query at?: uint32,

  /** Apply instead of dry run */
  @query commit?: boolean,

  /** Report leftover old-name spellings in plain text */
  @query text_refs?: boolean,

  /** SCIP index (default ROOT/index.scip): its seats join the plan */
  @valueName("INDEX") @query verify_scip?: path,

  /** Only report the SCIP diff; keep the syntax plan as is */
  @query no_scip_merge?: boolean,

  /** End with one JSON line of abstains */
  @query json?: boolean,
): EditPlan;

/** Run a tree-sitter query over files */
@route("/query")
@post
op query(
  /** Files, directories, or globs; - reads a path list from stdin */
  @skip @query paths?: string[],

  @bodyRoot inputs: Inputs,

  /** Language name (default: from each file's extension) */
  @query lang?: string,

  /** Tree-sitter query text */
  @query query: string,

  /** Expected content digest (one input only) */
  @query digest?: string,

  /** Write to a new SQLite database instead of stdout */
  @valueName("PATH") @query sqlite?: path,
): FactSummary;

/** Replace a generated region between sprefa markers */
@route("/region") @post
op region(
  /** DL7 file holding the markers */
  @path target: path,
  /** Marker id after sprefa:auto-begin / sprefa:auto-end */
  @path id: string,
  /** Generated body file, or - for stdin */
  @query generated: path = "-",
  /** Write the replacement (default: report drift) */
  @query apply?: boolean,
  /** Soopy state root for --apply */
  @query state?: path,
): EditPlan;

/** Stream fact deltas as the worktree changes */
@route("/watch") @post
op watch(
  /** Repository root (default: git root of the working directory) */
  @valueName("DIR") @query root?: path,
  /** Glob to watch; repeatable (default: every roster extension) */
  @encodedName("application/x-clap", "pattern") @valueName("GLOB") @query patterns: string[],
  /** Fact kinds (cst,type,call,df,data) */
  @valueDelimiter(",") @valueName("KINDS") @query kinds: string[],
  /** Receipt store path */
  @valueName("PATH") @query receipts?: path,
  /** Emit one snapshot and exit */
  @query once?: boolean,
  /** Poll interval when the platform watcher is unavailable */
  @minValue(1) @query poll_ms?: uint64 = 500,
): FactSummary;

/** Fact delta between two commits */
@route("/diff") @post
op diff(
  /** Repository root (default: git root of the working directory) */
  @valueName("DIR") @query root?: path,
  /** Base revision */
  @valueName("REV") @query from: string,
  /** Target revision */
  @valueName("REV") @query to: string,
  /** Glob to include; repeatable (default: every roster extension) */
  @encodedName("application/x-clap", "pattern") @valueName("GLOB") @query patterns: string[],
  /** Resolve arms (call,type) */
  @valueDelimiter(",") @valueName("ARMS") @query arms: string[],
  /** Write to a new SQLite database instead of stdout */
  @valueName("PATH") @query sqlite?: path,
): FactSummary;

/** Validate and re-emit foreign TSI JSONL */
@route("/ingest")
@post
op ingest(
  /** TSI JSONL files (/dev/stdin reads standard input) */
  @positional @valueName("PATH") @minItems(1) @query paths?: path[],

  @bodyRoot input: JsonlStream<TypeEdge>,

  @skip @header("X-Trace") trace?: string,

  /** Write to a new SQLite database instead of stdout */
  @valueName("PATH") @query sqlite?: path,
): FactSummary;

/** Print the output record schema */
@route("/schema") @post
op schema(): FactSummary;

/** Print the last N runs from the trail */
@route("/trail") @post
op trail(
  /** Runs to print */
  @valueName("N") @path runs: usize = 5,
): FactSummary;
