use std::path::PathBuf;

use clap::{ArgGroup, Args, Parser, Subcommand};

pub const AFTER_HELP: &str = concat!(
    "Logging: RUST_LOG (default sprefa_extract=info,hafley_scm=info), HAFLEY_LOG_FORMAT=json|text\n",
    "Build: git hash: ",
    env!("SPREFA_BUILD_GIT_HASH"),
    ", datetime: ",
    env!("SPREFA_BUILD_DATETIME"),
);

#[derive(Parser)]
#[command(
    name = "ryi",
    version,
    about = "Source files -> flat graph facts (JSONL on stdout, or --sqlite)",
    after_help = AFTER_HELP,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true,
    disable_help_subcommand = true
)]
pub struct Ryi {
    #[command(subcommand)]
    pub cmd: Option<Cmd>,

    #[command(flatten)]
    pub file: FileArgs,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Syntax-only whole-project facts (no compiler)
    Fast(FastArgs),
    /// The SCIP oracle written as fast's tables
    Slow(SlowArgs),
    /// Raw SCIP index rows
    Scip(ScipArgs),
    /// Ask one question of the resolved call/type graph
    Graph(GraphArgs),
    /// Move one item into another file, with its imports
    Cleave(CleaveArgs),
    /// Move a file and repair every specifier that names it
    Move(MoveArgs),
    /// Rename a symbol and every occurrence bound to it
    Rename(RenameArgs),
    /// Run a tree-sitter query over files
    Query(QueryArgs),
    /// Replace a generated region between sprefa markers
    Region(RegionArgs),
    /// Stream fact deltas as the worktree changes
    Watch(WatchArgs),
    /// Fact delta between two commits
    Diff(DiffArgs),
    /// Validate and re-emit foreign TSI JSONL
    Ingest(IngestArgs),
    /// Print the output record schema
    Schema,
    /// Print the last N runs from the trail
    Trail(TrailArgs),
}

/// Every file-taking verb flattens this.
#[derive(Args, Default, Clone)]
pub struct Inputs {
    /// Files, directories, or globs; - reads a path list from stdin
    #[arg(value_name = "PATH")]
    pub paths: Vec<String>,

    /// Keep files matching GLOB under each directory input; repeatable
    #[arg(long = "pattern", value_name = "GLOB")]
    pub patterns: Vec<String>,

    /// Run on the files FILE reaches over imports (universe: the PATH inputs, else FILE's project)
    #[arg(long, value_name = "FILE")]
    pub entry: Vec<PathBuf>,

    /// Import hops from --entry
    #[arg(long, value_name = "N", requires = "entry")]
    pub depth: Option<u32>,

    /// Corpus root
    #[arg(long, value_name = "DIR")]
    pub root: Option<PathBuf>,
}

/// `ryi PATH...`: per-file extraction and the flag-selected project modes.
#[derive(Args, Default)]
pub struct FileArgs {
    #[command(flatten)]
    pub inputs: Inputs,

    /// `inputs` expanded, filled after parsing.
    #[arg(skip)]
    pub paths: Vec<PathBuf>,

    /// Fact kinds per file (cst,type,call,df,data,cfg)
    #[arg(long, value_delimiter = ',', value_name = "KINDS")]
    pub kinds: Option<Vec<String>>,

    /// Resolve arms under --resolve (call,type,flow)
    #[arg(long, value_delimiter = ',', value_name = "ARMS", requires = "resolve")]
    pub arms: Option<Vec<String>>,

    /// Write to a new SQLite database instead of stdout
    #[arg(long, value_name = "PATH", conflicts_with = "bench")]
    pub sqlite: Option<PathBuf>,

    /// Time extraction and print per-family counts to stderr
    #[arg(long)]
    pub bench: bool,

    /// Resolve cross-file edges over all given paths
    #[arg(long, conflicts_with = "bench")]
    pub resolve: bool,

    /// Load this index.scip
    #[arg(long, value_name = "FILE", conflicts_with = "scip_build")]
    pub scip_index: Option<PathBuf>,

    /// Use rust-analyzer for Rust call/type targets
    #[arg(long, requires = "root")]
    pub rust_checker: bool,

    /// Use the TypeScript compiler for TS call/type targets
    #[arg(long, requires = "root")]
    pub ts_checker: bool,

    /// Use go/types for Go call/type targets
    #[arg(long, requires = "root")]
    pub go_checker: bool,

    /// Build the index with the language's indexer, then load it
    #[arg(long, requires = "root")]
    pub scip_build: bool,

    /// Seconds allowed for one indexer run under --scip-build
    #[arg(long, value_name = "SECS")]
    pub scip_timeout: Option<u64>,

    /// File-to-file edges folded from a SCIP index
    #[arg(long, requires = "root", conflicts_with_all = ["bench", "resolve", "file_fact"])]
    pub scip_deps: bool,

    /// File-to-file edges resolved from syntax, no index
    #[arg(long, requires = "root", conflicts_with_all = ["bench", "resolve", "scip_deps", "file_fact"])]
    pub deps: bool,

    /// Manifest-to-manifest workspace edges
    #[arg(long, requires = "root", conflicts_with_all = ["bench", "resolve", "scip_deps", "deps", "file_fact"])]
    pub package_deps: bool,

    /// Prepend one file record: path, digest, bytes, lines
    #[arg(long, conflicts_with_all = ["resolve"])]
    pub file_fact: bool,

    /// Add 1-based line/col beside every span
    #[arg(long)]
    pub lines: bool,

    /// Wrap output in the TSI envelope
    #[arg(long, conflicts_with_all = ["bench", "deps", "package_deps", "scip_deps", "file_fact"])]
    pub witness: bool,

    /// Skip inputs over this many bytes (0 = no limit)
    #[arg(long, value_name = "BYTES")]
    pub max_bytes: Option<u64>,
}

#[derive(Args)]
pub struct FastArgs {
    #[command(flatten)]
    pub inputs: Inputs,

    /// Write to a new SQLite database instead of stdout
    #[arg(long, value_name = "PATH")]
    pub sqlite: Option<PathBuf>,

    /// Add 1-based line/col beside every span
    #[arg(long)]
    pub lines: bool,
}

#[derive(Args)]
pub struct SlowArgs {
    #[command(flatten)]
    pub inputs: Inputs,

    /// Write to a new SQLite database instead of stdout
    #[arg(long, value_name = "PATH")]
    pub sqlite: Option<PathBuf>,

    /// Add 1-based line/col beside every span
    #[arg(long)]
    pub lines: bool,

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

#[derive(Args)]
pub struct ScipArgs {
    #[command(flatten)]
    pub inputs: Inputs,

    /// Write to a new SQLite database instead of stdout
    #[arg(long, value_name = "PATH")]
    pub sqlite: Option<PathBuf>,

    /// Add 1-based line/col beside every span
    #[arg(long)]
    pub lines: bool,

    /// Load this index.scip instead of finding or building one
    #[arg(long, value_name = "FILE", conflicts_with = "indexer")]
    pub scip_index: Option<PathBuf>,

    /// SCIP index cache directory
    #[arg(long, value_name = "DIR")]
    pub scip_cache: Option<PathBuf>,

    /// Seconds allowed for one indexer run
    #[arg(long, value_name = "SECS")]
    pub scip_timeout: Option<u64>,

    /// Run only this language's SCIP indexer
    #[arg(long, value_name = "LANG")]
    pub indexer: Option<String>,

    /// Stream the index records themselves instead of the scip_* relations
    #[arg(long)]
    pub raw: bool,

    /// Only these --raw record kinds (comma-separated)
    #[arg(long, value_name = "KINDS", requires = "raw")]
    pub records: Option<String>,

    /// Add the source text to each scip_occurrence
    #[arg(long, requires = "raw")]
    pub occurrence_text: bool,

    /// Build the index for --raw with the inputs' language indexer
    #[arg(long, requires_all = ["raw", "root"], conflicts_with = "scip_index")]
    pub scip_build: bool,
}

#[derive(Args)]
#[command(group(ArgGroup::new("arm").required(true).args(["callers", "uses", "from", "call_path", "type_path", "flow_path"])))]
pub struct GraphArgs {
    #[command(flatten)]
    pub inputs: Inputs,

    /// Resolved call edges landing on NAME
    #[arg(long, value_name = "NAME")]
    pub callers: Option<String>,

    /// Declarations that reference type NAME
    #[arg(long, value_name = "NAME")]
    pub uses: Option<String>,

    /// Everything NAME reaches along call edges
    #[arg(long, value_name = "NAME")]
    pub from: Option<String>,

    /// Shortest call paths from NAME
    #[arg(long, value_name = "NAME")]
    pub call_path: Option<String>,

    /// Shortest type-reference paths from NAME
    #[arg(long, value_name = "NAME")]
    pub type_path: Option<String>,

    /// Flow paths from BLOB@START:END
    #[arg(long, value_name = "BLOB@START:END")]
    pub flow_path: Option<String>,

    /// Keep the fact store in a new SQLite database at PATH
    #[arg(long, value_name = "PATH")]
    pub sqlite: Option<PathBuf>,

    /// Walk the SCIP oracle's edges (ryi slow) instead of the syntax resolve
    #[arg(long, conflicts_with_all = ["rust_checker", "ts_checker", "go_checker"])]
    pub slow: bool,

    /// Seconds the question may run; past it graph exits 3
    #[arg(long, value_name = "SECS", default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..))]
    pub timeout: u64,

    /// Query a committed revision
    #[arg(long, value_name = "REV", requires = "root", conflicts_with_all = ["rust_checker", "ts_checker", "go_checker", "scip_index", "sqlite"])]
    pub at: Option<String>,

    /// Diff the answers against REV
    #[arg(long, value_name = "REV", requires = "at", conflicts_with = "sqlite")]
    pub compare: Option<String>,

    /// Load this index.scip
    #[arg(long, value_name = "FILE", requires = "root")]
    pub scip_index: Option<PathBuf>,

    /// Add rust-analyzer type evidence
    #[arg(long, requires = "root")]
    pub rust_checker: bool,

    /// Add TypeScript checker type evidence
    #[arg(long, requires = "root")]
    pub ts_checker: bool,

    /// Add go/types type evidence
    #[arg(long, requires = "root")]
    pub go_checker: bool,
}

#[derive(Args)]
pub struct CleaveArgs {
    /// SRC#ITEM (omit with --list)
    pub target: Option<String>,

    /// Destination file, created if missing (omit with --list)
    pub dest: Option<PathBuf>,

    /// TSV of SRC#ITEM<TAB>DEST rows, applied in order as one stage
    #[arg(long, conflicts_with_all = ["target", "dest", "json"])]
    pub list: Option<PathBuf>,

    /// Corpus root (default: git root of SRC)
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Soopy state root, outside the corpus
    #[arg(long)]
    pub state: Option<PathBuf>,

    /// Also move private helpers only this item uses
    #[arg(long)]
    pub drag: bool,

    /// Apply instead of dry run
    #[arg(long)]
    pub commit: bool,

    /// Command to run after --commit; failure rolls back
    #[arg(long)]
    pub verify: Option<String>,

    /// Report leftover SRC spellings in plain text
    #[arg(long)]
    pub text_refs: bool,

    /// End with one JSON line holding the plan
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct MoveArgs {
    /// File to move (omit with --list)
    pub old: Option<PathBuf>,

    /// Destination
    pub new: Option<PathBuf>,

    /// TSV of old<TAB>new rows
    #[arg(long)]
    pub list: Option<PathBuf>,

    /// Corpus root; repeatable
    #[arg(long)]
    pub root: Vec<PathBuf>,

    /// Directory --verify runs in (default: first root)
    #[arg(long)]
    pub verify_cwd: Option<PathBuf>,

    /// Soopy state root, outside the corpus
    #[arg(long)]
    pub state: Option<PathBuf>,

    /// Apply instead of dry run
    #[arg(long)]
    pub commit: bool,

    /// Leave a reexport shim at OLD instead of rewriting importers
    #[arg(long)]
    pub shim: bool,

    /// Move a Rust module's `mod` line instead of adding #[path]
    #[arg(long)]
    pub relocate_mod: bool,

    /// Command to run after --commit; failure rolls back
    #[arg(long)]
    pub verify: Option<String>,

    /// Report leftover old-path spellings in plain text
    #[arg(long)]
    pub text_refs: bool,
}

#[derive(Args)]
#[command(after_help = "Exit codes: 2 plan error, 3 ambiguous (pass --at), 4 not found, 5 inexact, 6 dynamic, 7 plan has abstains")]
pub struct RenameArgs {
    /// FILE#OLD (omit with --list)
    pub target: Option<String>,

    /// New name
    pub new: Option<String>,

    /// TSV of anchor<TAB>old<TAB>new rows
    #[arg(long)]
    pub list: Option<PathBuf>,

    /// Corpus root (default: git root of the first anchor)
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Soopy state root, outside the corpus
    #[arg(long)]
    pub state: Option<PathBuf>,

    /// Byte offset of the declaration when OLD is declared twice
    #[arg(long)]
    pub at: Option<u32>,

    /// Apply instead of dry run
    #[arg(long)]
    pub commit: bool,

    /// Report leftover old-name spellings in plain text
    #[arg(long)]
    pub text_refs: bool,

    /// SCIP index (default ROOT/index.scip): its seats join the plan
    #[arg(long, value_name = "INDEX")]
    pub verify_scip: Option<PathBuf>,

    /// Only report the SCIP diff; keep the syntax plan as is
    #[arg(long)]
    pub no_scip_merge: bool,

    /// End with one JSON line of abstains
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct QueryArgs {
    #[command(flatten)]
    pub inputs: Inputs,

    /// Language name (default: from each file's extension)
    #[arg(long)]
    pub lang: Option<String>,

    /// Tree-sitter query text
    #[arg(long)]
    pub query: String,

    /// Expected content digest (one input only)
    #[arg(long)]
    pub digest: Option<String>,

    /// Write to a new SQLite database instead of stdout
    #[arg(long, value_name = "PATH")]
    pub sqlite: Option<PathBuf>,
}

#[derive(Args)]
pub struct RegionArgs {
    /// DL7 file holding the markers
    pub target: PathBuf,

    /// Marker id after sprefa:auto-begin / sprefa:auto-end
    pub id: String,

    /// Generated body file, or - for stdin
    #[arg(long, default_value = "-")]
    pub generated: PathBuf,

    /// Write the replacement (default: report drift)
    #[arg(long)]
    pub apply: bool,

    /// Soopy state root for --apply
    #[arg(long)]
    pub state: Option<PathBuf>,
}

#[derive(Args)]
pub struct WatchArgs {
    /// Repository root (default: git root of the working directory)
    #[arg(long, value_name = "DIR")]
    pub root: Option<PathBuf>,

    /// Glob to watch; repeatable (default: every roster extension)
    #[arg(long = "pattern", value_name = "GLOB")]
    pub patterns: Vec<String>,

    /// Fact kinds (cst,type,call,df,data)
    #[arg(long, value_delimiter = ',', value_name = "KINDS")]
    pub kinds: Vec<String>,

    /// Receipt store path
    #[arg(long, value_name = "PATH")]
    pub receipts: Option<PathBuf>,

    /// Emit one snapshot and exit
    #[arg(long)]
    pub once: bool,

    /// Poll interval when the platform watcher is unavailable
    #[arg(long, default_value_t = 500, value_parser = clap::value_parser!(u64).range(1..))]
    pub poll_ms: u64,
}

#[derive(Args)]
pub struct DiffArgs {
    /// Repository root (default: git root of the working directory)
    #[arg(long, value_name = "DIR")]
    pub root: Option<PathBuf>,

    /// Base revision
    #[arg(long, value_name = "REV")]
    pub from: String,

    /// Target revision
    #[arg(long, value_name = "REV")]
    pub to: String,

    /// Glob to include; repeatable (default: every roster extension)
    #[arg(long = "pattern", value_name = "GLOB")]
    pub patterns: Vec<String>,

    /// Resolve arms (call,type)
    #[arg(long, value_delimiter = ',', value_name = "ARMS")]
    pub arms: Vec<String>,

    /// Write to a new SQLite database instead of stdout
    #[arg(long, value_name = "PATH")]
    pub sqlite: Option<PathBuf>,
}

#[derive(Args)]
pub struct IngestArgs {
    /// TSI JSONL files (/dev/stdin reads standard input)
    #[arg(required = true, value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Write to a new SQLite database instead of stdout
    #[arg(long, value_name = "PATH")]
    pub sqlite: Option<PathBuf>,
}

#[derive(Args)]
pub struct TrailArgs {
    /// Runs to print
    #[arg(value_name = "N", default_value_t = 5)]
    pub runs: usize,
}
