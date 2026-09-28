import "@hafley/typespec-decorator-def"; // Daemon validator must load before @typespec/http.
import "@typespec/http";
import "@typespec/http/streams";
import "./domain.tsp";

using Http;
using Http.Streams;
using Clap;
using Daemon;

@service
@daemon(#{ idleSecs: 600, handshake: true, serverBin: "ryii" })
@rootArgs(FileArgs)
@argsConflictsWithSubcommands
@doc("Source files -> flat graph facts (JSONL on stdout, or --sqlite)")
@afterHelp("Logging: RUST_LOG (default sprefa_extract=info,hafley_scm=info), HAFLEY_LOG_FORMAT=json|text\nCapabilities: `ryii capabilities` prints the declared language matrix.\nSQLite with --resolve stores phase 1 per-file and phase 2 project rows; stdout streams phase 2 only. Every SQLite unresolved row has its path.\nBuild: git hash: $SPREFA_BUILD_GIT_HASH, datetime: $SPREFA_BUILD_DATETIME")
namespace Ryi;

/** Unmodified stdin bytes, including newlines and non-JSON content. */
model RawByteStream is HttpStream<bytes, "application/octet-stream", bytes>;

/** Declared language and tooling capabilities */
op capabilities(): JsonlStream<jsonValue>;

/** Extract file facts with the root flags */
op extract(args: FileArgs, @bodyRoot input: RawByteStream): JsonlStream<jsonValue>;

/** Syntax-only whole-project facts (no compiler) */
op fast(
  ...Inputs,
  @bodyRoot input: RawByteStream,

  /** Write to a new SQLite database instead of stdout */
  @valueName("PATH") sqlite?: path,

  /** Add 1-based line/col beside every span */
  lines?: boolean,
): JsonlStream<jsonValue>;

/** The SCIP oracle written as fast's tables */
op slow(
  ...Inputs,
  @bodyRoot input: RawByteStream,

  /** Write to a new SQLite database instead of stdout */
  @valueName("PATH") sqlite?: path,

  /** Add 1-based line/col beside every span */
  lines?: boolean,

  /** Load this index.scip instead of finding or building one */
  @valueName("FILE") scip_index?: path,

  /** Skip the compiler checkers */
  no_checker?: boolean,

  /** Seconds allowed for one indexer run */
  @valueName("SECS") scip_timeout?: uint64,
): JsonlStream<jsonValue>;

/** Raw SCIP index rows */
op scip(
  ...Inputs,
  @bodyRoot input: RawByteStream,

  /** Write to a new SQLite database instead of stdout */
  @valueName("PATH") sqlite?: path,

  /** Add 1-based line/col beside every span */
  lines?: boolean,

  /** Load this index.scip instead of finding or building one */
  @valueName("FILE") @conflictsWith(#["indexer"]) scip_index?: path,

  /** SCIP index cache directory */
  @valueName("DIR") scip_cache?: path,

  /** Seconds allowed for one indexer run */
  @valueName("SECS") scip_timeout?: uint64,

  /** Run only this language's SCIP indexer */
  @valueName("LANG") indexer?: string,

  /** Stream the index records themselves instead of the scip_* relations */
  raw?: boolean,

  /** Only these --raw record kinds (comma-separated) */
  @valueName("KINDS") @requires("raw") records?: string,

  /** Add the source text to each scip_occurrence */
  @requires("raw") occurrence_text?: boolean,

  /** Build the index for --raw with the inputs' language indexer */
  @requiresAll(#["raw", "root"]) @conflictsWith(#["scip_index"]) scip_build?: boolean,
): JsonlStream<jsonValue>;

/** Ask one question of the resolved call/type graph */
@requiredOneOf(#["callers", "uses", "from", "call_path", "type_path", "flow_path", "slice"], "arm")
op graph(
  ...Inputs,
  @bodyRoot input: RawByteStream,

  /** Resolved call edges landing on NAME */
  @valueName("NAME") callers?: string,

  /** Declarations that reference type NAME */
  @valueName("NAME") uses?: string,

  /** Everything NAME reaches along call edges */
  @valueName("NAME") from?: string,

  /** Shortest call paths from NAME */
  @valueName("NAME") call_path?: string,

  /** Shortest type-reference paths from NAME */
  @valueName("NAME") type_path?: string,

  @doc("Flow paths from BLOB@START:END")
  @valueName("BLOB@START:END") flow_path?: string,

  /** Control-dependence slice at PATH:BYTE */
  @valueName("PATH:BYTE") slice?: string,

  /** Keep the fact store in a new SQLite database at PATH */
  @valueName("PATH") sqlite?: path,

  /** Walk the SCIP oracle's edges (ryi slow) instead of the syntax resolve */
  @conflictsWith(#["rust_checker", "ts_checker", "go_checker"]) slow?: boolean,

  /** Seconds the question may run; past it graph exits 3 */
  @minValue(1) @valueName("SECS") timeout?: uint64 = 30,

  /** Query a committed revision */
  @valueName("REV") @requires("root") @conflictsWith(#["rust_checker", "ts_checker", "go_checker", "scip_index", "sqlite"]) at?: string,

  /** Diff the answers against REV */
  @valueName("REV") @requires("at") @conflictsWith(#["sqlite"]) compare?: string,

  /** Load this index.scip */
  @valueName("FILE") @requires("root") scip_index?: path,

  /** Add rust-analyzer type evidence */
  @requires("root") rust_checker?: boolean,

  /** Add TypeScript checker type evidence */
  @requires("root") ts_checker?: boolean,

  /** Add go/types type evidence */
  @requires("root") go_checker?: boolean,
): JsonlStream<jsonValue>;

/** Move one item into another file, with its imports */
op cleave(
  /** SRC#ITEM (omit with --list) */
  @positional target?: string,

  /** Destination file, created if missing (omit with --list) */
  @positional dest?: path,

  /** TSV of SRC#ITEM<TAB>DEST rows, applied in order as one stage */
  @conflictsWith(#["target", "dest", "json"]) list?: path,

  /** Corpus root (default: git root of SRC) */
  root?: path,

  /** Soopy state root, outside the corpus */
  state?: path,

  /** Also move private helpers only this item uses */
  drag?: boolean,

  /** Apply instead of dry run */
  commit?: boolean,

  /** Command to run after --commit; failure rolls back */
  verify?: string,

  /** Report leftover SRC spellings in plain text */
  text_refs?: boolean,

  /** End with one JSON line holding the plan */
  json?: boolean,
): jsonValue;

/** Move a file and repair every specifier that names it */
op `move`(
  /** File to move (omit with --list) */
  @positional old?: path,

  /** Destination */
  @positional new?: path,

  /** TSV of old<TAB>new rows */
  list?: path,

  /** Corpus root; repeatable */
  root?: path[],

  /** Directory --verify runs in (default: first root) */
  verify_cwd?: path,

  /** Soopy state root, outside the corpus */
  state?: path,

  /** Apply instead of dry run */
  commit?: boolean,

  /** Leave a reexport shim at OLD instead of rewriting importers */
  shim?: boolean,

  /** Move a Rust module's `mod` line instead of adding #[path] */
  relocate_mod?: boolean,

  /** Command to run after --commit; failure rolls back */
  verify?: string,

  /** Report leftover old-path spellings in plain text */
  text_refs?: boolean,
): jsonValue;

/** Rename a symbol and every occurrence bound to it */
@afterHelp("Exit codes: 2 plan error, 3 ambiguous (pass --at), 4 not found, 5 inexact, 6 dynamic, 7 plan has abstains")
op rename(
  /** FILE#OLD (omit with --list) */
  @positional target?: string,

  /** New name */
  @positional new?: string,

  /** TSV of anchor<TAB>old<TAB>new rows */
  list?: path,

  /** Corpus root (default: git root of the first anchor) */
  root?: path,

  /** Soopy state root, outside the corpus */
  state?: path,

  /** Byte offset of the declaration when OLD is declared twice */
  at?: uint32,

  /** Use the TypeScript 7 checker for TypeScript rename */
  slow?: boolean,

  /** Apply instead of dry run */
  commit?: boolean,

  /** Report leftover old-name spellings in plain text */
  text_refs?: boolean,

  /** SCIP index (default ROOT/index.scip): its seats join the plan */
  @valueName("INDEX") verify_scip?: path,

  /** Only report the SCIP diff; keep the syntax plan as is */
  no_scip_merge?: boolean,

  /** End with one JSON line of abstains */
  json?: boolean,
): jsonValue;

/** Run a tree-sitter query over files */
op query(
  ...Inputs,
  @bodyRoot input: RawByteStream,

  /** Language name (default: from each file's extension) */
  lang?: string,

  /** Tree-sitter query text */
  query: string,

  /** Expected content digest (one input only) */
  digest?: string,

  /** Write to a new SQLite database instead of stdout */
  @valueName("PATH") sqlite?: path,
): JsonlStream<jsonValue>;

/** Replace a generated region between sprefa markers */
op region(
  @bodyRoot input: RawByteStream,
  /** DL7 file holding the markers */
  @positional target: path,
  /** Marker id after sprefa:auto-begin / sprefa:auto-end */
  @positional id: string,
  /** Generated body file, or - for stdin */
  generated: path = "-",
  /** Write the replacement (default: report drift) */
  apply?: boolean,
  /** Soopy state root for --apply */
  state?: path,
): jsonValue;

/** Stream fact deltas as the worktree changes */
op watch(
  /** Repository root (default: git root of the working directory) */
  @valueName("DIR") root?: path,
  /** Glob to watch; repeatable (default: every roster extension) */
  @encodedName("application/x-clap", "pattern") @valueName("GLOB") patterns: string[],
  /** Fact kinds (cst,type,call,df,data) */
  @valueDelimiter(",") kinds: string[],
  /** Receipt store path */
  @valueName("PATH") receipts?: path,
  /** Emit one snapshot and exit */
  once?: boolean,
  /** Poll interval when the platform watcher is unavailable */
  @minValue(1) poll_ms?: uint64 = 500,
): JsonlStream<jsonValue>;

/** Fact delta between two commits */
op diff(
  /** Repository root (default: git root of the working directory) */
  @valueName("DIR") root?: path,
  /** Base revision */
  @valueName("REV") from: string,
  /** Target revision */
  @valueName("REV") to: string,
  /** Glob to include; repeatable (default: every roster extension) */
  @encodedName("application/x-clap", "pattern") @valueName("GLOB") patterns: string[],
  /** Resolve arms (call,type) */
  @valueDelimiter(",") arms: string[],
  /** Write to a new SQLite database instead of stdout */
  @valueName("PATH") sqlite?: path,
): JsonlStream<jsonValue>;

/** Validate and re-emit foreign TSI JSONL */
op ingest(
  /** TSI JSONL files (/dev/stdin reads standard input) */
  @query @positional @valueName("PATH") @minItems(1) paths?: path[],

  @bodyRoot input: RawByteStream,

  @skip @header("X-Trace") trace?: string,

  /** Write to a new SQLite database instead of stdout */
  @query @valueName("PATH") sqlite?: path,
): jsonValue;

/** Print the output record schema */
op schema(): jsonValue;

/** Print the last N runs from the trail */
op trail(
  /** Runs to print */
  @positional @valueName("N") runs: usize = 5,
): jsonValue;

/** Rank files from entrypoints and propose dependency-first numeric prefixes */
op stratify(
  ...Inputs,

  /** Entry point PATH[:NAME][=RANK]; repeatable */
  @valueName("PATH[:NAME][=RANK]")
  from: string[],

  /** Base line target; default is the corpus median */
  @valueName("N")
  base_lines?: uint32,

  /** Edge plane used to build the graph (call, type, or both) */
  @valueName("KIND")
  kind?: string = "both",
): JsonlStream<jsonValue>;
