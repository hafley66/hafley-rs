use std::path::PathBuf;

use crate::models::inputs::Inputs;

#[derive(Debug)]
pub struct OpError(pub String);

impl<E: std::error::Error> From<E> for OpError {
    fn from(e: E) -> Self {
        OpError(e.to_string())
    }
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub type OpResult<T> = Result<T, OpError>;

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct FastArgs {
  #[doc = "Files, directories, or globs; - reads a path list from stdin"]
  #[arg(skip)]
  pub paths: Vec<String>,
  #[command(flatten)]
  pub inputs: Inputs,
  #[doc = "Write to a new SQLite database instead of stdout"]
  #[arg(long, value_name = "PATH")]
  pub sqlite: Option<PathBuf>,
  #[doc = "Add 1-based line/col beside every span"]
  #[arg(long)]
  pub lines: bool,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct SlowArgs {
  #[doc = "Files, directories, or globs; - reads a path list from stdin"]
  #[arg(skip)]
  pub paths: Vec<String>,
  #[command(flatten)]
  pub inputs: Inputs,
  #[doc = "Write to a new SQLite database instead of stdout"]
  #[arg(long, value_name = "PATH")]
  pub sqlite: Option<PathBuf>,
  #[doc = "Add 1-based line/col beside every span"]
  #[arg(long)]
  pub lines: bool,
  #[doc = "Load this index.scip instead of finding or building one"]
  #[arg(long, value_name = "FILE")]
  pub scip_index: Option<PathBuf>,
  #[doc = "Skip the compiler checkers"]
  #[arg(long)]
  pub no_checker: bool,
  #[doc = "Seconds allowed for one indexer run"]
  #[arg(long, value_name = "SECS")]
  pub scip_timeout: Option<u64>,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct ScipArgs {
  #[doc = "Files, directories, or globs; - reads a path list from stdin"]
  #[arg(skip)]
  pub paths: Vec<String>,
  #[command(flatten)]
  pub inputs: Inputs,
  #[doc = "Write to a new SQLite database instead of stdout"]
  #[arg(long, value_name = "PATH")]
  pub sqlite: Option<PathBuf>,
  #[doc = "Add 1-based line/col beside every span"]
  #[arg(long)]
  pub lines: bool,
  #[doc = "Load this index.scip instead of finding or building one"]
  #[arg(long, value_name = "FILE", conflicts_with = "indexer")]
  pub scip_index: Option<PathBuf>,
  #[doc = "SCIP index cache directory"]
  #[arg(long, value_name = "DIR")]
  pub scip_cache: Option<PathBuf>,
  #[doc = "Seconds allowed for one indexer run"]
  #[arg(long, value_name = "SECS")]
  pub scip_timeout: Option<u64>,
  #[doc = "Run only this language's SCIP indexer"]
  #[arg(long, value_name = "LANG")]
  pub indexer: Option<String>,
  #[doc = "Stream the index records themselves instead of the scip_* relations"]
  #[arg(long)]
  pub raw: bool,
  #[doc = "Only these --raw record kinds (comma-separated)"]
  #[arg(long, value_name = "KINDS", requires = "raw")]
  pub records: Option<String>,
  #[doc = "Add the source text to each scip_occurrence"]
  #[arg(long, requires = "raw")]
  pub occurrence_text: bool,
  #[doc = "Build the index for --raw with the inputs' language indexer"]
  #[arg(long, requires_all = ["raw", "root"], conflicts_with = "scip_index")]
  pub scip_build: bool,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
#[command(group(clap::ArgGroup::new("arm").required(true).args(["callers", "uses", "from", "call_path", "type_path", "flow_path"])))]
pub struct GraphArgs {
  #[doc = "Files, directories, or globs; - reads a path list from stdin"]
  #[arg(skip)]
  pub paths: Vec<String>,
  #[command(flatten)]
  pub inputs: Inputs,
  #[doc = "Resolved call edges landing on NAME"]
  #[arg(long, value_name = "NAME")]
  pub callers: Option<String>,
  #[doc = "Declarations that reference type NAME"]
  #[arg(long, value_name = "NAME")]
  pub uses: Option<String>,
  #[doc = "Everything NAME reaches along call edges"]
  #[arg(long, value_name = "NAME")]
  pub from: Option<String>,
  #[doc = "Shortest call paths from NAME"]
  #[arg(long, value_name = "NAME")]
  pub call_path: Option<String>,
  #[doc = "Shortest type-reference paths from NAME"]
  #[arg(long, value_name = "NAME")]
  pub type_path: Option<String>,
  #[doc = "Flow paths from BLOB@START:END"]
  #[arg(long, value_name = "BLOB@START:END")]
  pub flow_path: Option<String>,
  #[doc = "Keep the fact store in a new SQLite database at PATH"]
  #[arg(long, value_name = "PATH")]
  pub sqlite: Option<PathBuf>,
  #[doc = "Walk the SCIP oracle's edges (ryi slow) instead of the syntax resolve"]
  #[arg(long, conflicts_with_all = ["rust_checker", "ts_checker", "go_checker"])]
  pub slow: bool,
  #[doc = "Seconds the question may run; past it graph exits 3"]
  #[arg(long, default_value_t = 30, value_name = "SECS", value_parser = clap::value_parser!(u64).range(1..))]
  pub timeout: u64,
  #[doc = "Query a committed revision"]
  #[arg(long, value_name = "REV", requires = "root", conflicts_with_all = ["rust_checker", "ts_checker", "go_checker", "scip_index", "sqlite"])]
  pub at: Option<String>,
  #[doc = "Diff the answers against REV"]
  #[arg(long, value_name = "REV", requires = "at", conflicts_with = "sqlite")]
  pub compare: Option<String>,
  #[doc = "Load this index.scip"]
  #[arg(long, value_name = "FILE", requires = "root")]
  pub scip_index: Option<PathBuf>,
  #[doc = "Add rust-analyzer type evidence"]
  #[arg(long, requires = "root")]
  pub rust_checker: bool,
  #[doc = "Add TypeScript checker type evidence"]
  #[arg(long, requires = "root")]
  pub ts_checker: bool,
  #[doc = "Add go/types type evidence"]
  #[arg(long, requires = "root")]
  pub go_checker: bool,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct CleaveArgs {
  #[doc = "SRC#ITEM (omit with --list)"]
  #[arg()]
  pub target: Option<String>,
  #[doc = "Destination file, created if missing (omit with --list)"]
  #[arg()]
  pub dest: Option<PathBuf>,
  #[doc = "TSV of SRC#ITEM<TAB>DEST rows, applied in order as one stage"]
  #[arg(long, conflicts_with_all = ["target", "dest", "json"])]
  pub list: Option<PathBuf>,
  #[doc = "Corpus root (default: git root of SRC)"]
  #[arg(long)]
  pub root: Option<PathBuf>,
  #[doc = "Soopy state root, outside the corpus"]
  #[arg(long)]
  pub state: Option<PathBuf>,
  #[doc = "Also move private helpers only this item uses"]
  #[arg(long)]
  pub drag: bool,
  #[doc = "Apply instead of dry run"]
  #[arg(long)]
  pub commit: bool,
  #[doc = "Command to run after --commit; failure rolls back"]
  #[arg(long)]
  pub verify: Option<String>,
  #[doc = "Report leftover SRC spellings in plain text"]
  #[arg(long)]
  pub text_refs: bool,
  #[doc = "End with one JSON line holding the plan"]
  #[arg(long)]
  pub json: bool,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct MoveArgs {
  #[doc = "File to move (omit with --list)"]
  #[arg()]
  pub old: Option<PathBuf>,
  #[doc = "Destination"]
  #[arg()]
  pub new: Option<PathBuf>,
  #[doc = "TSV of old<TAB>new rows"]
  #[arg(long)]
  pub list: Option<PathBuf>,
  #[doc = "Corpus root; repeatable"]
  #[arg(long)]
  pub root: Vec<PathBuf>,
  #[doc = "Directory --verify runs in (default: first root)"]
  #[arg(long)]
  pub verify_cwd: Option<PathBuf>,
  #[doc = "Soopy state root, outside the corpus"]
  #[arg(long)]
  pub state: Option<PathBuf>,
  #[doc = "Apply instead of dry run"]
  #[arg(long)]
  pub commit: bool,
  #[doc = "Leave a reexport shim at OLD instead of rewriting importers"]
  #[arg(long)]
  pub shim: bool,
  #[doc = "Move a Rust module's `mod` line instead of adding #[path]"]
  #[arg(long)]
  pub relocate_mod: bool,
  #[doc = "Command to run after --commit; failure rolls back"]
  #[arg(long)]
  pub verify: Option<String>,
  #[doc = "Report leftover old-path spellings in plain text"]
  #[arg(long)]
  pub text_refs: bool,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct RenameArgs {
  #[doc = "FILE#OLD (omit with --list)"]
  #[arg()]
  pub target: Option<String>,
  #[doc = "New name"]
  #[arg()]
  pub new: Option<String>,
  #[doc = "TSV of anchor<TAB>old<TAB>new rows"]
  #[arg(long)]
  pub list: Option<PathBuf>,
  #[doc = "Corpus root (default: git root of the first anchor)"]
  #[arg(long)]
  pub root: Option<PathBuf>,
  #[doc = "Soopy state root, outside the corpus"]
  #[arg(long)]
  pub state: Option<PathBuf>,
  #[doc = "Byte offset of the declaration when OLD is declared twice"]
  #[arg(long)]
  pub at: Option<u32>,
  #[doc = "Apply instead of dry run"]
  #[arg(long)]
  pub commit: bool,
  #[doc = "Report leftover old-name spellings in plain text"]
  #[arg(long)]
  pub text_refs: bool,
  #[doc = "SCIP index (default ROOT/index.scip): its seats join the plan"]
  #[arg(long, value_name = "INDEX")]
  pub verify_scip: Option<PathBuf>,
  #[doc = "Only report the SCIP diff; keep the syntax plan as is"]
  #[arg(long)]
  pub no_scip_merge: bool,
  #[doc = "End with one JSON line of abstains"]
  #[arg(long)]
  pub json: bool,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct QueryArgs {
  #[doc = "Files, directories, or globs; - reads a path list from stdin"]
  #[arg(skip)]
  pub paths: Vec<String>,
  #[command(flatten)]
  pub inputs: Inputs,
  #[doc = "Language name (default: from each file's extension)"]
  #[arg(long)]
  pub lang: Option<String>,
  #[doc = "Tree-sitter query text"]
  #[arg(long)]
  pub query: String,
  #[doc = "Expected content digest (one input only)"]
  #[arg(long)]
  pub digest: Option<String>,
  #[doc = "Write to a new SQLite database instead of stdout"]
  #[arg(long, value_name = "PATH")]
  pub sqlite: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct RegionArgs {
  #[doc = "DL7 file holding the markers"]
  #[arg()]
  pub target: PathBuf,
  #[doc = "Marker id after sprefa:auto-begin / sprefa:auto-end"]
  #[arg()]
  pub id: String,
  #[doc = "Generated body file, or - for stdin"]
  #[arg(long, default_value = "-")]
  pub generated: PathBuf,
  #[doc = "Write the replacement (default: report drift)"]
  #[arg(long)]
  pub apply: bool,
  #[doc = "Soopy state root for --apply"]
  #[arg(long)]
  pub state: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct WatchArgs {
  #[doc = "Repository root (default: git root of the working directory)"]
  #[arg(long, value_name = "DIR")]
  pub root: Option<PathBuf>,
  #[doc = "Glob to watch; repeatable (default: every roster extension)"]
  #[arg(long = "pattern", value_name = "GLOB")]
  pub patterns: Vec<String>,
  #[doc = "Fact kinds (cst,type,call,df,data)"]
  #[arg(long, value_name = "KINDS", value_delimiter = ',')]
  pub kinds: Vec<String>,
  #[doc = "Receipt store path"]
  #[arg(long, value_name = "PATH")]
  pub receipts: Option<PathBuf>,
  #[doc = "Emit one snapshot and exit"]
  #[arg(long)]
  pub once: bool,
  #[doc = "Poll interval when the platform watcher is unavailable"]
  #[arg(long, default_value_t = 500, value_parser = clap::value_parser!(u64).range(1..))]
  pub poll_ms: u64,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct DiffArgs {
  #[doc = "Repository root (default: git root of the working directory)"]
  #[arg(long, value_name = "DIR")]
  pub root: Option<PathBuf>,
  #[doc = "Base revision"]
  #[arg(long, value_name = "REV")]
  pub from: String,
  #[doc = "Target revision"]
  #[arg(long, value_name = "REV")]
  pub to: String,
  #[doc = "Glob to include; repeatable (default: every roster extension)"]
  #[arg(long = "pattern", value_name = "GLOB")]
  pub patterns: Vec<String>,
  #[doc = "Resolve arms (call,type)"]
  #[arg(long, value_name = "ARMS", value_delimiter = ',')]
  pub arms: Vec<String>,
  #[doc = "Write to a new SQLite database instead of stdout"]
  #[arg(long, value_name = "PATH")]
  pub sqlite: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct IngestArgs {
  #[doc = "TSI JSONL files (/dev/stdin reads standard input)"]
  #[arg(value_name = "PATH", required = true)]
  pub paths: Vec<PathBuf>,
  #[arg(skip)]
  pub trace: Option<String>,
  #[doc = "Write to a new SQLite database instead of stdout"]
  #[arg(long, value_name = "PATH")]
  pub sqlite: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, Default)]
pub struct SchemaArgs {}

#[derive(clap::Args, Debug, Clone, serde::Serialize)]
pub struct TrailArgs {
  #[doc = "Runs to print"]
  #[arg(default_value_t = 5, value_name = "N")]
  pub runs: usize,
}
