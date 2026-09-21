// Generated from schema/3_cli.tsp by a one-off compile (binding-core emitRust).
// Not wired into `just gen` or crates/sprefa-extract/src/bin/ryi.rs. Do not edit by hand.
// alloy-imports-start
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;
use clap::{Parser, Subcommand};

// alloy-imports-end
// alloy-row-structs-start

// alloy-row-structs-end
// alloy-extraction-structs-start

// alloy-extraction-structs-end
// alloy-upsert-fns-start

// alloy-upsert-fns-end
// alloy-extract-fns-start

// alloy-extract-fns-end
// alloy-cli-start

#[derive(Debug, Clone, Parser)]
#[command(about = "sprefa-extract: one source file -> flat graph facts (JSONL to stdout)")]
pub struct RyiCliCli {

      #[arg(index = 0, help = "A source file to extract; output goes to stdout unless --bench")]
      pub paths: String,
      #[arg(long, help = "Which kinds of facts to extract, comma-separated: cst, type, call, df, data, cfg")]
      pub family: Option<String>,
      #[arg(long, help = "Write facts to a NEW SQLite database at PATH, then print schema/query commands")]
      pub sqlite: Option<String>,
      #[arg(long, help = "Time extract + flatten and report per-family counts to stderr")]
      pub bench: bool,
      #[arg(long, help = "Resolve cross-file edges across all supplied paths (see --family)")]
      pub resolve: bool,
      #[arg(long, help = "Root that SCIP document paths are relative to; also the --scip-build root")]
      pub project_root: Option<String>,
      #[arg(long, help = "Load a prebuilt index.scip into the resolve context")]
      pub scip_index: Option<String>,
      #[arg(long, help = "Answer rust call/type destinations with rust-analyzer's own resolution")]
      pub rust_checker: bool,
      #[arg(long, help = "Answer ts call/type destinations with the TypeScript compiler's own resolution")]
      pub ts_checker: bool,
      #[arg(long, help = "Answer go call/type destinations with go/types' own resolution")]
      pub go_checker: bool,
      #[arg(long, help = "Build the index with the language's own indexer, then load it")]
      pub scip_build: bool,
      #[arg(long, help = "Stream file-to-file dependency edges folded from a SCIP index")]
      pub scip_deps: bool,
      #[arg(long, help = "Stream the whole SCIP index as facts, every field the protobuf carries")]
      pub scip_facts: bool,
      #[arg(long, help = "Narrow --scip-facts to a comma-separated list of record kinds")]
      pub scip_record: Option<String>,
      #[arg(long, help = "Also carry the source slice at each scip_occurrence span, as text")]
      pub occurrence_text: bool,
      #[arg(long, help = "Stream file_edge rows resolved syntactically, with no SCIP index")]
      pub deps: bool,
      #[arg(long, help = "Stream package_edge rows: workspace-internal manifest-to-manifest edges")]
      pub package_deps: bool,
      #[arg(long, help = "Prepend one file record: path, content digest, byte count, line count")]
      pub file_fact: bool,
      #[arg(long, help = "Decorate stdout: 1-based line and col beside every start/end span")]
      pub lines: bool,
      #[arg(long, help = "Wrap the stream in the TSI envelope")]
      pub witness: bool,
      #[arg(long, help = "Read foreign TSI JSONL, validate it against the relation registry, and re-emit it")]
      pub ingest: String,
      #[arg(long, help = "Byte ceiling for one input; over it emits size_skip and exits 0 (0 = none)")]
      pub max_bytes: Option<u64>,
      #[arg(long, help = "Ast-grep pattern in ID=PATTERN form; repeat to batch patterns over one parse")]
      pub ast_pattern: String,
      #[arg(long, help = "Contextual pattern selector in ID=KIND form; repeat at most once per query")]
      pub ast_selector: String,
      #[arg(long, help = "Single-node metavariable to emit in ID=NAME form; repeat per query")]
      pub ast_capture: String,
      #[arg(long, help = "Where --family scip places and finds its index cache")]
      pub scip_cache: Option<String>,
      #[arg(long, help = "Wall budget in seconds for ONE indexer run under --family scip")]
      pub scip_timeout: Option<u64>,
      #[arg(long, help = "Run ONE named SCIP indexer under --family scip instead of every roster match")]
      pub indexer: Option<String>,
      #[arg(long, help = "Print the JSONL output contract to stdout and exit (no extraction)")]
      pub schema: bool,
      #[arg(long, help = "Print the last N runs of the on-disk trail and exit (no extraction)")]
      pub trail: Option<u64>,
      #[command(subcommand)]
      pub command: RyiCliSubcommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum RyiCliSubcommand {
  /// alias: pins --family diet_scip, then parses the same flags as the root command
  Fast(FastCli),
  /// alias: pins --family scip, then parses the same flags as the root command
  Slow(SlowCli),
  /// ask one question of the resolved call and type graph of a corpus
  Graph(GraphCli),
  /// move one item out of a file into another, with the imports it needs
  Cleave(CleaveCli),
  /// move a file and repair every specifier that named it
  Move(MoveCli),
  /// rename a symbol and respell every occurrence bound to it
  Rename(RenameCli),
  Query(QueryCli),
  /// stream fact deltas for a working tree as it changes
  Watch(WatchCli),
  /// the fact delta between two commits, one shot
  Diff(DiffCli),
  Region(RegionCli),
}
#[derive(Debug, Clone, Parser)]
#[command(about = "alias: pins --family diet_scip, then parses the same flags as the root command")]
pub struct FastCli {

      #[arg(index = 0, help = "A source file to extract; output goes to stdout unless --bench")]
      pub paths: String,
      #[arg(long, help = "Write facts to a NEW SQLite database at PATH, then print schema/query commands")]
      pub sqlite: Option<String>,
      #[arg(long, help = "Time extract + flatten and report per-family counts to stderr")]
      pub bench: bool,
      #[arg(long, help = "Resolve cross-file edges across all supplied paths (see --family)")]
      pub resolve: bool,
      #[arg(long, help = "Root that SCIP document paths are relative to; also the --scip-build root")]
      pub project_root: Option<String>,
      #[arg(long, help = "Build the index with the language's own indexer, then load it")]
      pub scip_build: bool,
      #[arg(long, help = "Stream file-to-file dependency edges folded from a SCIP index")]
      pub scip_deps: bool,
      #[arg(long, help = "Stream the whole SCIP index as facts, every field the protobuf carries")]
      pub scip_facts: bool,
      #[arg(long, help = "Narrow --scip-facts to a comma-separated list of record kinds")]
      pub scip_record: Option<String>,
      #[arg(long, help = "Also carry the source slice at each scip_occurrence span, as text")]
      pub occurrence_text: bool,
      #[arg(long, help = "Stream file_edge rows resolved syntactically, with no SCIP index")]
      pub deps: bool,
      #[arg(long, help = "Stream package_edge rows: workspace-internal manifest-to-manifest edges")]
      pub package_deps: bool,
      #[arg(long, help = "Prepend one file record: path, content digest, byte count, line count")]
      pub file_fact: bool,
      #[arg(long, help = "Decorate stdout: 1-based line and col beside every start/end span")]
      pub lines: bool,
      #[arg(long, help = "Wrap the stream in the TSI envelope")]
      pub witness: bool,
      #[arg(long, help = "Read foreign TSI JSONL, validate it against the relation registry, and re-emit it")]
      pub ingest: String,
      #[arg(long, help = "Byte ceiling for one input; over it emits size_skip and exits 0 (0 = none)")]
      pub max_bytes: Option<u64>,
      #[arg(long, help = "Ast-grep pattern in ID=PATTERN form; repeat to batch patterns over one parse")]
      pub ast_pattern: String,
      #[arg(long, help = "Contextual pattern selector in ID=KIND form; repeat at most once per query")]
      pub ast_selector: String,
      #[arg(long, help = "Single-node metavariable to emit in ID=NAME form; repeat per query")]
      pub ast_capture: String,
      #[arg(long, help = "Where --family scip places and finds its index cache")]
      pub scip_cache: Option<String>,
      #[arg(long, help = "Wall budget in seconds for ONE indexer run under --family scip")]
      pub scip_timeout: Option<u64>,
      #[arg(long, help = "Print the JSONL output contract to stdout and exit (no extraction)")]
      pub schema: bool,
      #[arg(long, help = "Print the last N runs of the on-disk trail and exit (no extraction)")]
      pub trail: Option<u64>,
}
#[derive(Debug, Clone, Parser)]
#[command(about = "alias: pins --family scip, then parses the same flags as the root command")]
pub struct SlowCli {

      #[arg(index = 0, help = "A source file to extract; output goes to stdout unless --bench")]
      pub paths: String,
      #[arg(long, help = "Write facts to a NEW SQLite database at PATH, then print schema/query commands")]
      pub sqlite: Option<String>,
      #[arg(long, help = "Time extract + flatten and report per-family counts to stderr")]
      pub bench: bool,
      #[arg(long, help = "Resolve cross-file edges across all supplied paths (see --family)")]
      pub resolve: bool,
      #[arg(long, help = "Root that SCIP document paths are relative to; also the --scip-build root")]
      pub project_root: Option<String>,
      #[arg(long, help = "Build the index with the language's own indexer, then load it")]
      pub scip_build: bool,
      #[arg(long, help = "Stream file-to-file dependency edges folded from a SCIP index")]
      pub scip_deps: bool,
      #[arg(long, help = "Stream the whole SCIP index as facts, every field the protobuf carries")]
      pub scip_facts: bool,
      #[arg(long, help = "Narrow --scip-facts to a comma-separated list of record kinds")]
      pub scip_record: Option<String>,
      #[arg(long, help = "Also carry the source slice at each scip_occurrence span, as text")]
      pub occurrence_text: bool,
      #[arg(long, help = "Stream file_edge rows resolved syntactically, with no SCIP index")]
      pub deps: bool,
      #[arg(long, help = "Stream package_edge rows: workspace-internal manifest-to-manifest edges")]
      pub package_deps: bool,
      #[arg(long, help = "Prepend one file record: path, content digest, byte count, line count")]
      pub file_fact: bool,
      #[arg(long, help = "Decorate stdout: 1-based line and col beside every start/end span")]
      pub lines: bool,
      #[arg(long, help = "Wrap the stream in the TSI envelope")]
      pub witness: bool,
      #[arg(long, help = "Read foreign TSI JSONL, validate it against the relation registry, and re-emit it")]
      pub ingest: String,
      #[arg(long, help = "Byte ceiling for one input; over it emits size_skip and exits 0 (0 = none)")]
      pub max_bytes: Option<u64>,
      #[arg(long, help = "Ast-grep pattern in ID=PATTERN form; repeat to batch patterns over one parse")]
      pub ast_pattern: String,
      #[arg(long, help = "Contextual pattern selector in ID=KIND form; repeat at most once per query")]
      pub ast_selector: String,
      #[arg(long, help = "Single-node metavariable to emit in ID=NAME form; repeat per query")]
      pub ast_capture: String,
      #[arg(long, help = "Where --family scip places and finds its index cache")]
      pub scip_cache: Option<String>,
      #[arg(long, help = "Wall budget in seconds for ONE indexer run under --family scip")]
      pub scip_timeout: Option<u64>,
      #[arg(long, help = "Run ONE named SCIP indexer under --family scip instead of every roster match")]
      pub indexer: Option<String>,
      #[arg(long, help = "Print the JSONL output contract to stdout and exit (no extraction)")]
      pub schema: bool,
      #[arg(long, help = "Print the last N runs of the on-disk trail and exit (no extraction)")]
      pub trail: Option<u64>,
}
#[derive(Debug, Clone, Parser)]
#[command(about = "ask one question of the resolved call and type graph of a corpus")]
pub struct GraphCli {

      #[arg(index = 0, help = "Files and directories; a directory is walked by the language roster")]
      pub paths: String,
      #[arg(long, help = "Who calls NAME: one row per resolved call edge landing on it")]
      pub callers: Option<String>,
      #[arg(long, help = "Who references the type NAME: one row per referencing declaration")]
      pub uses: Option<String>,
      #[arg(long, help = "What NAME reaches along resolved call edges, transitively")]
      pub from: Option<String>,
      #[arg(long, help = "Publish the fact store as DIR/graph.db instead of keeping it in memory")]
      pub state: Option<String>,
      #[arg(long, help = "Drop the stderr summary line; stdout is JSONL either way")]
      pub json: bool,
}
#[derive(Debug, Clone, Parser)]
#[command(about = "move one item out of a file into another, with the imports it needs")]
pub struct CleaveCli {

      #[arg(index = 0, help = "<SRC>#<ITEM>: the file the item is declared in and its name")]
      pub target: String,
      #[arg(index = 1, help = "The file it lands in; created when it does not exist")]
      pub dest: String,
      #[arg(long, help = "Corpus root; defaults to the git root holding SRC")]
      pub root: Option<String>,
      #[arg(long, help = "Soopy state root; must sit outside the corpus root")]
      pub state: Option<String>,
      #[arg(long, help = "Move the same-file private helpers only the item uses")]
      pub drag: bool,
      #[arg(long, help = "Apply the plan to the real tree instead of dry running it")]
      pub commit: bool,
      #[arg(long, help = "Run this shell command in the root after --commit; rolls back on failure")]
      pub verify: Option<String>,
      #[arg(long, help = "Report the SRC spellings this cleave leaves behind in plain text")]
      pub text_refs: bool,
      #[arg(long, help = "Close the output with one JSON line carrying the whole plan")]
      pub json: bool,
}
#[derive(Debug, Clone, Parser)]
#[command(about = "move a file and repair every specifier that named it")]
pub struct MoveCli {

      #[arg(index = 0, help = "The file to rehome; omitted when --list carries the moves")]
      pub old: Option<String>,
      #[arg(index = 1, help = "Where it lands")]
      pub new: Option<String>,
      #[arg(long, help = "A tsv of old<TAB>new rows, one move per line")]
      pub list: Option<String>,
      #[arg(long, help = "Corpus root; repeatable, each root gets its own plan and stage batch")]
      pub root: String,
      #[arg(long, help = "Directory the --verify command runs in; defaults to the first root")]
      pub verify_cwd: Option<String>,
      #[arg(long, help = "Soopy state root; must sit outside the corpus root")]
      pub state: Option<String>,
      #[arg(long, help = "Apply the plan to the real tree instead of dry running it")]
      pub commit: bool,
      #[arg(long, help = "Leave a reexport shim behind at old instead of rewriting importers")]
      pub shim: bool,
      #[arg(long, help = "Relocate a moved Rust module's mod declaration instead of adding #[path]")]
      pub relocate_mod: bool,
      #[arg(long, help = "Run this shell command in the move root after --commit; rolls back on failure")]
      pub verify: Option<String>,
      #[arg(long, help = "Report the old-path spellings this move leaves behind in plain text")]
      pub text_refs: bool,
}
#[derive(Debug, Clone, Parser)]
#[command(about = "rename a symbol and respell every occurrence bound to it")]
pub struct RenameCli {

      #[arg(index = 0, help = "<FILE>#<OLD>: the declaring file and the identifier as written today")]
      pub target: Option<String>,
      #[arg(index = 1, help = "What the identifier becomes")]
      pub new: Option<String>,
      #[arg(long, help = "A tsv of anchor<TAB>old<TAB>new rows, one rename per line")]
      pub list: Option<String>,
      #[arg(long, help = "Corpus root; defaults to the git root holding the first anchor")]
      pub root: Option<String>,
      #[arg(long, help = "Soopy state root; must sit outside the corpus root")]
      pub state: Option<String>,
      #[arg(long, help = "Byte offset inside the declaration, when the anchor declares OLD more than once")]
      pub at: Option<u32>,
      #[arg(long, help = "Apply the plan to the real tree instead of dry running it")]
      pub commit: bool,
      #[arg(long, help = "Report the old-name spellings this rename leaves behind in plain text")]
      pub text_refs: bool,
      #[arg(long, help = "Cross-check the plan against a prebuilt SCIP index; report only")]
      pub verify_scip: Option<String>,
      #[arg(long, help = "Close the output with one JSON line naming the sites the arm declined to plan")]
      pub json: bool,
}
#[derive(Debug, Clone, Parser)]
pub struct QueryCli {

      #[arg(long)]
      pub lang: String,
      #[arg(long)]
      pub query: String,
      #[arg(long)]
      pub digest: Option<String>,
      #[arg(index = 0)]
      pub path: String,
}
#[derive(Debug, Clone, Parser)]
#[command(about = "stream fact deltas for a working tree as it changes")]
pub struct WatchCli {

      #[arg(index = 0, help = "Corpus root to watch")]
      pub root: String,
      #[arg(long, help = "Glob to watch; repeatable, defaults to the built-in language globs")]
      pub pattern: String,
      #[arg(long, help = "Comma-separated family names, same vocabulary as the root command")]
      pub family: Option<String>,
      #[arg(long, help = "Receipt-store state path")]
      pub state: Option<String>,
      #[arg(long, help = "Emit one snapshot and exit instead of watching")]
      pub once: bool,
      #[arg(long, default_value = "500", help = "Poll interval in milliseconds when the platform watcher is unavailable")]
      pub poll_ms: u32,
}
#[derive(Debug, Clone, Parser)]
#[command(about = "the fact delta between two commits, one shot")]
pub struct DiffCli {

      #[arg(index = 0, help = "Corpus root")]
      pub root: String,
      #[arg(long, help = "Revision to diff from (required)")]
      pub from: String,
      #[arg(long, help = "Revision to diff to (required)")]
      pub to: String,
      #[arg(long, help = "Glob to include; repeatable, defaults to the watch verb's language globs")]
      pub pattern: String,
      #[arg(long, help = "Resolve arms to diff: call and/or type; defaults to both")]
      pub family: Option<String>,
      #[arg(long, help = "Write rows to a NEW SQLite database at PATH instead of stdout JSONL")]
      pub sqlite: Option<String>,
      #[arg(long, help = "No-op: JSONL is the only wire this verb writes")]
      pub json: bool,
}
#[derive(Debug, Clone, Parser)]
pub struct RegionCli {

      #[arg(index = 0, help = "DL7 file containing the owned comment markers")]
      pub target: String,
      #[arg(index = 1, help = "Marker identifier following sprefa:auto-begin and sprefa:auto-end")]
      pub id: String,
      #[arg(long, default_value = "-", help = "Generated body file, or - for stdin")]
      pub generated: String,
      #[arg(long, help = "Commit the content-guarded replacement; without it, report drift")]
      pub apply: bool,
      #[arg(long, help = "Soopy state root used by an applied mutation")]
      pub state: Option<String>,
}
// alloy-cli-end

// Custom code below this line is preserved across re-generation.
