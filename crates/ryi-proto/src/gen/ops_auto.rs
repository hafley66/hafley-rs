use std::path::PathBuf;

use crate::models::edit::Edit;
use crate::models::file_args::FileArgs;
use crate::models::inputs::Inputs;
use crate::models::spans::Spans;
use crate::models::store::Store;

#[derive(Debug)]
pub struct OpError(pub String, pub i32);

impl<E: std::error::Error> From<E> for OpError {
    fn from(e: E) -> Self {
        OpError(e.to_string(), 1)
    }
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub type OpResult<T> = Result<T, OpError>;

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct CapabilitiesArgs {}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractArgs {
  #[command(flatten)]
  #[serde(flatten)]
  pub args: FileArgs,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FastArgs {
  #[doc = "Resolve written receiver types in fast mode"]
  #[arg(long, value_name = "written")]
  pub fast_receivers: Option<String>,
  #[command(flatten)]
  #[serde(flatten)]
  pub inputs: Inputs,
  #[command(flatten)]
  #[serde(flatten)]
  pub store: Store,
  #[command(flatten)]
  #[serde(flatten)]
  pub spans: Spans,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SlowArgs {
  #[command(flatten)]
  #[serde(flatten)]
  pub inputs: Inputs,
  #[command(flatten)]
  #[serde(flatten)]
  pub store: Store,
  #[command(flatten)]
  #[serde(flatten)]
  pub spans: Spans,
  #[doc = "Also project this saved index.scip; slow never builds one"]
  #[arg(long, value_name = "FILE")]
  pub scip_index: Option<PathBuf>,
  #[doc = "Skip the compiler checkers; needs --scip-index"]
  #[arg(long)]
  #[serde(default)]
  pub no_checker: bool,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScipArgs {
  #[command(flatten)]
  #[serde(flatten)]
  pub inputs: Inputs,
  #[command(flatten)]
  #[serde(flatten)]
  pub store: Store,
  #[command(flatten)]
  #[serde(flatten)]
  pub spans: Spans,
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
  #[serde(default)]
  pub raw: bool,
  #[doc = "Only these --raw record kinds (comma-separated)"]
  #[arg(long, value_name = "KINDS", requires = "raw")]
  pub records: Option<String>,
  #[doc = "Add the source text to each scip_occurrence"]
  #[arg(long, requires = "raw")]
  #[serde(default)]
  pub occurrence_text: bool,
  #[doc = "Build the index for --raw with the inputs' language indexer"]
  #[arg(long, requires_all = ["raw", "root"], conflicts_with = "scip_index")]
  #[serde(default)]
  pub scip_build: bool,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
#[command(group(clap::ArgGroup::new("arm").required(true).args(["callers", "uses", "from", "call_path", "type_path", "flow_path", "slice"])))]
pub struct GraphArgs {
  #[doc = "Resolve written receiver types in fast mode"]
  #[arg(long, value_name = "written")]
  pub fast_receivers: Option<String>,
  #[command(flatten)]
  #[serde(flatten)]
  pub inputs: Inputs,
  #[doc = "Resolved call edges landing on NAME; PATH#NAME keeps NAME declared in files whose path ends with PATH"]
  #[arg(long, value_name = "[PATH#]NAME")]
  pub callers: Option<String>,
  #[doc = "Declarations that reference type NAME"]
  #[arg(long, value_name = "NAME")]
  pub uses: Option<String>,
  #[doc = "Everything NAME reaches along call edges; PATH#NAME seeds only NAME declared in files whose path ends with PATH"]
  #[arg(long, value_name = "[PATH#]NAME")]
  pub from: Option<String>,
  #[doc = "Shortest call paths from NAME; PATH#NAME seeds only NAME declared in files whose path ends with PATH"]
  #[arg(long, value_name = "[PATH#]NAME")]
  pub call_path: Option<String>,
  #[doc = "Shortest type-reference paths from NAME; PATH#NAME seeds only NAME declared in files whose path ends with PATH"]
  #[arg(long, value_name = "[PATH#]NAME")]
  pub type_path: Option<String>,
  #[doc = "Flow paths from PATH@START:END or BLOB@START:END. START/END are zero-based UTF-8 byte offsets, END exclusive. PATH is read under --root (or the current directory); --at reads the revision. BLOB is the exact tagged content digest from file.digest or flow_edge.from_blob (blake3:HEX for worktree facts), not a bare git object ID. Follows local df edges and interprocedural flow_edge facts from the asked node only. --reverse answers what reaches the node."]
  #[arg(long, value_name = "BLOB@START:END")]
  pub flow_path: Option<String>,
  #[doc = "Follow incoming value-flow edges to answer what reaches the seed"]
  #[arg(long, requires = "flow_path")]
  #[serde(default)]
  pub reverse: bool,
  #[doc = "Control-dependence slice at PATH:BYTE"]
  #[arg(long, value_name = "PATH:BYTE")]
  pub slice: Option<String>,
  #[command(flatten)]
  #[serde(flatten)]
  pub store: Store,
  #[doc = "Answer from the compiler-backed tier (ryi slow) instead of the syntax resolve"]
  #[arg(long, conflicts_with_all = ["rust_checker", "ts_checker", "go_checker"])]
  #[serde(default)]
  pub slow: bool,
  #[doc = "Seconds the question may run; past it graph exits 3"]
  #[arg(long, default_value_t = 30, value_name = "SECS", value_parser = clap::value_parser!(u64).range(1..))]
  #[serde(default = "__serde_default_graph_timeout")]
  pub timeout: u64,
  #[doc = "Answer at REV, or diff the answers from BASE to TARGET"]
  #[arg(long, value_name = "REV|BASE..TARGET", conflicts_with_all = ["rust_checker", "ts_checker", "go_checker", "scip_index", "sqlite"])]
  pub rev: Option<String>,
  #[doc = "Load this index.scip"]
  #[arg(long, value_name = "FILE", requires = "root")]
  pub scip_index: Option<PathBuf>,
  #[doc = "Add rust-analyzer type evidence"]
  #[arg(long, requires = "root")]
  #[serde(default)]
  pub rust_checker: bool,
  #[doc = "Add TypeScript checker type evidence"]
  #[arg(long, requires = "root")]
  #[serde(default)]
  pub ts_checker: bool,
  #[doc = "Add go/types type evidence"]
  #[arg(long, requires = "root")]
  #[serde(default)]
  pub go_checker: bool,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CleaveArgs {
  #[doc = "SRC#ITEM (omit with --list)"]
  #[arg()]
  pub target: Option<String>,
  #[doc = "Destination file, created if missing (omit with --list)"]
  #[arg()]
  pub dest: Option<PathBuf>,
  #[doc = "TSV of SRC#ITEM<TAB>DEST rows, applied in order as one stage; positionals become PATH scope"]
  #[arg(long, value_name = "FILE")]
  pub list: Option<PathBuf>,
  #[command(flatten)]
  #[serde(flatten)]
  pub inputs: Inputs,
  #[command(flatten)]
  #[serde(flatten)]
  pub edit: Edit,
  #[doc = "Also move private helpers only this item uses"]
  #[arg(long)]
  #[serde(default)]
  pub drag: bool,
  #[doc = "Plan with the compiler-backed tier: rust-analyzer for Rust, the TypeScript LSP for TS"]
  #[arg(long)]
  #[serde(default)]
  pub slow: bool,
  #[doc = "Command to run after --apply; failure rolls back"]
  #[arg(long)]
  pub verify: Option<String>,
  #[doc = "End with one JSON line holding the plan"]
  #[arg(long)]
  #[serde(default)]
  pub json: bool,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MoveArgs {
  #[doc = "File to move (omit with --list)"]
  #[arg()]
  pub old: Option<PathBuf>,
  #[doc = "Destination"]
  #[arg()]
  pub new: Option<PathBuf>,
  #[doc = "TSV of old<TAB>new rows; positionals become PATH scope"]
  #[arg(long, value_name = "FILE")]
  pub list: Option<PathBuf>,
  #[command(flatten)]
  #[serde(flatten)]
  pub inputs: Inputs,
  #[command(flatten)]
  #[serde(flatten)]
  pub edit: Edit,
  #[doc = "Plan each DIR as its own corpus in one stage; repeatable"]
  #[arg(long, value_name = "DIR", conflicts_with = "root")]
  #[serde(default)]
  pub roots: Vec<PathBuf>,
  #[doc = "Directory --verify runs in (default: the first root)"]
  #[arg(long, value_name = "DIR")]
  pub verify_cwd: Option<PathBuf>,
  #[doc = "Leave a reexport shim at OLD instead of rewriting importers"]
  #[arg(long)]
  #[serde(default)]
  pub shim: bool,
  #[doc = "Move a Rust module's `mod` line instead of adding #[path]"]
  #[arg(long)]
  #[serde(default)]
  pub relocate_mod: bool,
  #[doc = "Command to run after --apply; failure rolls back"]
  #[arg(long)]
  pub verify: Option<String>,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenameArgs {
  #[doc = "FILE#OLD (omit with --list)"]
  #[arg()]
  pub target: Option<String>,
  #[doc = "New name"]
  #[arg()]
  pub new: Option<String>,
  #[doc = "TSV of anchor<TAB>old<TAB>new rows; positionals become PATH scope"]
  #[arg(long, value_name = "FILE")]
  pub list: Option<PathBuf>,
  #[command(flatten)]
  #[serde(flatten)]
  pub inputs: Inputs,
  #[command(flatten)]
  #[serde(flatten)]
  pub edit: Edit,
  #[doc = "Byte offset of the declaration when OLD is declared twice"]
  #[arg(long, value_name = "BYTE")]
  pub offset: Option<u32>,
  #[doc = "Plan with the compiler-backed tier: the TypeScript 7 checker for TS, rust-analyzer for Rust"]
  #[arg(long)]
  #[serde(default)]
  pub slow: bool,
  #[doc = "Load this index.scip (default ROOT/index.scip); its seats join the plan"]
  #[arg(long, value_name = "FILE")]
  pub scip_index: Option<PathBuf>,
  #[doc = "Only report the SCIP diff; keep the syntax plan as is"]
  #[arg(long)]
  #[serde(default)]
  pub no_scip_merge: bool,
  #[doc = "End with one JSON line of abstains"]
  #[arg(long)]
  #[serde(default)]
  pub json: bool,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QueryArgs {
  #[command(flatten)]
  #[serde(flatten)]
  pub inputs: Inputs,
  #[doc = "Language name (default: from each file's extension)"]
  #[arg(long)]
  pub lang: Option<String>,
  #[doc = "Tree-sitter query text"]
  #[arg(long)]
  pub query: Option<String>,
  #[doc = "scm++ query file: nested relations lowered to SQL over capture and CST rows"]
  #[arg(long, value_name = "FILE")]
  pub scmpp: Option<PathBuf>,
  #[doc = "Expected content digest (one input only)"]
  #[arg(long)]
  pub digest: Option<String>,
  #[command(flatten)]
  #[serde(flatten)]
  pub store: Store,
  #[doc = "Seconds the scm++ run may take; past it query exits 3"]
  #[arg(long, value_name = "SECS", requires = "scmpp", value_parser = clap::value_parser!(u64).range(1..))]
  pub timeout: Option<u64>,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegionArgs {
  #[doc = "DL7 file holding the markers"]
  #[arg()]
  pub target: PathBuf,
  #[doc = "Marker id after sprefa:auto-begin / sprefa:auto-end"]
  #[arg()]
  pub id: String,
  #[doc = "Generated body file, or - for stdin"]
  #[arg(long, default_value = "-")]
  #[serde(default = "__serde_default_region_generated")]
  pub generated: PathBuf,
  #[doc = "Write the replacement (default: report drift)"]
  #[arg(long)]
  #[serde(default)]
  pub apply: bool,
  #[doc = "Soopy state root, outside the corpus"]
  #[arg(long, value_name = "DIR")]
  pub state: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WatchArgs {
  #[doc = "Repository root (default: git root of the working directory)"]
  #[arg(long, value_name = "DIR")]
  pub root: Option<PathBuf>,
  #[doc = "Glob to watch; repeatable (default: every roster extension)"]
  #[arg(long = "pattern", value_name = "GLOB")]
  #[serde(default)]
  pub patterns: Vec<String>,
  #[doc = "Fact kinds (cst,type,call,df,data,cfg; default all but cst)"]
  #[arg(long, value_name = "KINDS", value_delimiter = ',')]
  #[serde(default)]
  pub kinds: Vec<String>,
  #[doc = "Receipt store path"]
  #[arg(long, value_name = "PATH")]
  pub receipts: Option<PathBuf>,
  #[doc = "Emit one snapshot and exit"]
  #[arg(long)]
  #[serde(default)]
  pub once: bool,
  #[doc = "Poll interval when the platform watcher is unavailable"]
  #[arg(long, default_value_t = 500, value_parser = clap::value_parser!(u64).range(1..))]
  #[serde(default = "__serde_default_watch_poll_ms")]
  pub poll_ms: u64,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiffArgs {
  #[doc = "Repository root (default: git root of the working directory)"]
  #[arg(long, value_name = "DIR")]
  pub root: Option<PathBuf>,
  #[doc = "Fact delta from BASE to TARGET"]
  #[arg(long, value_name = "REV|BASE..TARGET")]
  pub rev: String,
  #[doc = "Glob to include; repeatable (default: every roster extension)"]
  #[arg(long = "pattern", value_name = "GLOB")]
  #[serde(default)]
  pub patterns: Vec<String>,
  #[doc = "Resolve arms (call,type,flow; default call,type)"]
  #[arg(long, value_name = "ARMS", value_delimiter = ',')]
  #[serde(default)]
  pub arms: Vec<String>,
  #[command(flatten)]
  #[serde(flatten)]
  pub store: Store,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IngestArgs {
  #[doc = "TSI JSONL files (/dev/stdin reads standard input)"]
  #[arg(value_name = "PATH", required = true)]
  #[serde(default)]
  pub paths: Vec<PathBuf>,
  #[arg(skip)]
  #[serde(skip)]
  pub trace: Option<String>,
  #[doc = "Write the fact store to a new SQLite database at PATH"]
  #[arg(long, value_name = "PATH")]
  pub sqlite: Option<PathBuf>,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct SchemaArgs {}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrailArgs {
  #[doc = "Runs to print"]
  #[arg(default_value_t = 5, value_name = "N")]
  #[serde(default = "__serde_default_trail_runs")]
  pub runs: usize,
}

#[derive(clap::Args, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StratifyArgs {
  #[command(flatten)]
  #[serde(flatten)]
  pub inputs: Inputs,
  #[doc = "Entry point to rank from, PATH[#NAME][=RANK]; repeatable"]
  #[arg(long, value_name = "PATH[#NAME][=RANK]")]
  #[serde(default)]
  pub rank: Vec<String>,
  #[doc = "Base line target; default is the corpus median"]
  #[arg(long, value_name = "N")]
  pub base_lines: Option<u32>,
  #[doc = "Edge plane used to build the graph (call, type, or both)"]
  #[arg(long, default_value = "both", value_name = "KIND")]
  #[serde(default = "__serde_default_stratify_kind")]
  pub kind: String,
}

fn __serde_default_graph_timeout() -> u64 { serde_json::from_value(serde_json::json!(30)).expect("TypeSpec default matches Rust field") }

fn __serde_default_region_generated() -> PathBuf { serde_json::from_value(serde_json::json!("-")).expect("TypeSpec default matches Rust field") }

fn __serde_default_watch_poll_ms() -> u64 { serde_json::from_value(serde_json::json!(500)).expect("TypeSpec default matches Rust field") }

fn __serde_default_trail_runs() -> usize { serde_json::from_value(serde_json::json!(5)).expect("TypeSpec default matches Rust field") }

fn __serde_default_stratify_kind() -> String { serde_json::from_value(serde_json::json!("both")).expect("TypeSpec default matches Rust field") }
