import "@hafley/typespec-decorator-def";
using Clap;

namespace Ryi;

scalar path extends string;
scalar usize extends uint64;
scalar jsonValue extends string;

model Inputs {
  /** Files, directories, or globs; - reads a path list from stdin */
  @positional
  @valueName("PATH")
  paths: string[];

  /** Keep files matching GLOB under each directory input; repeatable */
  @encodedName("application/x-clap", "pattern")
  @valueName("GLOB")
  patterns: string[];

  /** Run on the files FILE reaches over imports (universe: the PATH inputs, else FILE's project) */
  @valueName("FILE")
  entry: path[];

  /** Import hops from --entry */
  @valueName("N")
  @requires("entry")
  depth?: uint32;

  /** Corpus root */
  @valueName("DIR")
  root?: path;
}

model FileArgs {
  inputs: Inputs;
  @hidden @global format?: string;
  @skip paths: path[];
  /** Fact kinds per file (cst,type,call,df,data,cfg) */
  @valueDelimiter(",") @valueName("KINDS") kinds?: string[];
  /** Resolve arms under --resolve (call,type,flow) */
  @valueDelimiter(",") @valueName("ARMS") @requires("resolve") arms?: string[];
  /** Write to a new SQLite database instead of stdout */
  @valueName("PATH") @conflictsWith(#["bench"]) sqlite?: path;
  /** Time extraction and print per-family counts to stderr */
  bench: boolean;
  /** Resolve cross-file edges over all given paths */
  @conflictsWith(#["bench"]) resolve: boolean;
  /** Load this index.scip */
  @valueName("FILE") @conflictsWith(#["scip_build"]) scip_index?: path;
  /** Use rust-analyzer for Rust call/type targets */
  @requires("root") rust_checker: boolean;
  /** Use the TypeScript compiler for TS call/type targets */
  @requires("root") ts_checker: boolean;
  /** Use go/types for Go call/type targets */
  @requires("root") go_checker: boolean;
  /** Build the index with the language's indexer, then load it */
  @requires("root") scip_build: boolean;
  /** Seconds allowed for one indexer run under --scip-build */
  @valueName("SECS") scip_timeout?: uint64;
  /** File-to-file edges folded from a SCIP index */
  @requires("root") @conflictsWith(#["bench", "resolve", "file_fact"]) scip_deps: boolean;
  /** File-to-file edges resolved from syntax, no index */
  @requires("root") @conflictsWith(#["bench", "resolve", "scip_deps", "file_fact"]) deps: boolean;
  /** Manifest-to-manifest workspace edges */
  @requires("root") @conflictsWith(#["bench", "resolve", "scip_deps", "deps", "file_fact"]) package_deps: boolean;
  /** Prepend one file record: path, digest, bytes, lines */
  @conflictsWith(#["resolve"]) file_fact: boolean;
  /** Add 1-based line/col beside every span */
  lines: boolean;
  /** Wrap output in the TSI envelope */
  @conflictsWith(#["bench", "deps", "package_deps", "scip_deps", "file_fact"]) witness: boolean;
  /** Skip inputs over this many bytes (0 = no limit) */
  @valueName("BYTES") max_bytes?: uint64;
}

enum TypeEdgeKind {
  Field: "field",
  Variant: "variant",
  Impl: "impl",
  Generic: "generic",
  Param: "param",
  Returns: "returns",
  Uses: "uses",
  DocRef: "doc_ref",
}

model TypeEdge {
  owner_path: path;
  owner_name?: string;
  owner_start: uint32;
  owner_end: uint32;
  target_path: path;
  target_name?: string;
  kind: TypeEdgeKind;
  resolution_origin: string;
}

model CallEdge {
  caller_path: path;
  caller_site_start: uint32;
  callee_path: path;
  callee_start: uint32;
  callee_end: uint32;
  resolution_origin: string;
}

model FactSummary {
  rows: uint64;
  tables: uint32;
}

model EditPlan {
  files: path[];
  edits: uint32;
  committed: boolean;
}
