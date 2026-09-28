# Serena evaluation: rename and references against ryi

## API matrix (installed Serena 1.7.0; binary and MCP)

The first table lists all 29 MCP tool schemas returned by stdio `tools/list` under `--context desktop-app`; these schemas were serialized from the server and preserve parameter types, required fields, and defaults. Optional integrations discovered by `serena tools list --all` but not activated in the LSP context are listed separately. Serena CLI help and dashboard routes follow. Runtime scenarios are complete only for `find_referencing_symbols`; other MCP endpoints and HTTP routes remain pending.

| Surface | Endpoint | Parameters / flags | Runtime scenarios |
|---|---|---|---|
| MCP | `create_text_file` | `relative_path` string required; `content` string required | pending |
| MCP | `replace_content` | `relative_path` string required; `needle` string required; `repl` string required; `mode` string enum(literal,regex) required; `allow_multiple_occurrences` boolean optional default=False | pending |
| MCP | `replace_in_files` | `needle` string required; `repl` string required; `mode` string enum(literal,regex) required; `relative_path` string optional default=''; `paths_include_glob` string optional default=''; `paths_exclude_glob` string optional default=''; `dry_run` boolean optional default=False; `occurrence_ids` array<string>|null optional default=None; `expected_count` integer optional default=-1; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `replace_symbol_body` | `name_path` string required; `relative_path` string required; `body` string required | pending |
| MCP | `insert_after_symbol` | `name_path` string required; `relative_path` string required; `body` string required | pending |
| MCP | `insert_before_symbol` | `name_path` string required; `relative_path` string required; `body` string required | pending |
| MCP | `read_file` | `relative_path` string required; `start_line` integer optional default=0; `end_line` integer|null optional default=None; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `list_dir` | `relative_path` string required; `recursive` boolean required; `skip_ignored_files` boolean optional default=False; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `find_file` | `file_mask` string required; `relative_path` string required | pending |
| MCP | `search_for_pattern` | `substring_pattern` string required; `context_lines_before` integer optional default=0; `context_lines_after` integer optional default=0; `paths_include_glob` string optional default=''; `paths_exclude_glob` string optional default=''; `relative_path` string optional default=''; `restrict_search_to_code_files` boolean optional default=False; `skip_ignored_files` boolean optional default=True; `multiline` boolean optional default=True; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `get_symbols_overview` | `relative_path` string required; `depth` integer optional default=-1; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `find_symbol` | `name_path_pattern` string required; `depth` integer optional default=0; `relative_path` string optional default=''; `include_body` boolean optional default=False; `include_info` boolean optional default=False; `include_kinds` array<integer> optional default=[]; `exclude_kinds` array<integer> optional default=[]; `substring_matching` boolean optional default=False; `max_matches` integer optional default=-1; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `find_referencing_symbols` | `name_path` string required; `relative_path` string required; `include_kinds` array<integer> optional default=[]; `exclude_kinds` array<integer> optional default=[]; `max_answer_chars` integer optional default=-1 | 9 calls below |
| MCP | `find_implementations` | `name_path` string required; `relative_path` string required; `include_info` boolean optional default=False; `include_kinds` array<integer> optional default=[]; `exclude_kinds` array<integer> optional default=[]; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `find_declaration` | `relative_path` string required; `regex` string required; `containing_symbol_name_path` string|null optional default=None; `include_body` boolean optional default=False; `include_info` boolean optional default=False | pending |
| MCP | `get_diagnostics_for_file` | `relative_path` string required; `start_line` integer optional default=0; `end_line` integer optional default=-1; `min_severity` integer optional default=4; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `rename_symbol` | `name_path` string required; `relative_path` string required; `new_name` string required | pending |
| MCP | `safe_delete_symbol` | `name_path_pattern` string required; `relative_path` string required | pending |
| MCP | `write_memory` | `memory_name` string required; `content` string required; `max_chars` integer optional default=-1 | pending |
| MCP | `read_memory` | `memory_name` string required | pending |
| MCP | `list_memories` | `topic` string optional default='' | pending |
| MCP | `delete_memory` | `memory_name` string required | pending |
| MCP | `rename_memory` | `old_name` string required; `new_name` string required | pending |
| MCP | `edit_memory` | `memory_name` string required; `needle` string required; `repl` string required; `mode` string enum(literal,regex) required; `allow_multiple_occurrences` boolean optional default=False | pending |
| MCP | `execute_shell_command` | `command` string required; `cwd` string|null optional default=None; `capture_stderr` boolean optional default=True; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `activate_project` | `project` string required | pending |
| MCP | `get_current_config` | none | pending |
| MCP | `onboarding` | none | pending |
| MCP | `initial_instructions` | none | pending |

Optional MCP integrations listed by `serena tools list --all` but not activated by the LSP server configuration, so not evaluated as callable endpoints: `delete_lines`, `get_diagnostics_for_symbol`, `insert_at_line`, `jet_brains_debug`, `jet_brains_find_declaration`, `jet_brains_find_implementations`, `jet_brains_find_referencing_symbols`, `jet_brains_find_symbol`, `jet_brains_get_symbols_overview`, `jet_brains_inline_symbol`, `jet_brains_list_inspections`, `jet_brains_move`, `jet_brains_rename`, `jet_brains_safe_delete`, `jet_brains_type_hierarchy`, `list_queryable_projects`, `open_dashboard`, `query_project`, `remove_project`, `replace_lines`, `restart_language_server`, `serena_info`. Their descriptions are in `serena tools list --all`; they have no `tools/list` input schema in the active server context.

### Runtime matrix rows: `find_referencing_symbols`

MCP server command: `serena start-mcp-server --project <project> --context claude-code --language-backend LSP --enable-web-dashboard False --open-web-dashboard False`. Calls below used `tools/call` with the listed tool and argument object. Samples are trimmed to one output line. Serena reports `content_around_reference` line numbers at 0-based values in Rust; site scoring adds 1 before comparing with SCIP lines.

| Project / scenario | MCP call args | rc / wall s | Trimmed Serena output sample | Serena vs SCIP sites | Closest ryi call and result |
|---|---|---:|---|---|---|
| `runs/serena/hafley-rs`; fn `push_claude_agent` | `{"name_path":"push_claude_agent","relative_path":"crates/boop-harness/src/harness/claude.rs"}` | 0 / 122.361 | `claude.rs:Function/parse_claude_agent_worktrees @ 888, 898` | exact, 2/2 sites; 1/1 files | `ryi graph --callers push_claude_agent --root <hafley-rs> <hafley-rs>`; rc=0, 2 edges at 889,899, exact |
| `runs/serena/hafley-rs`; type `Subscribe` | `{"name_path":"Subscribe","relative_path":"crates/boop/src/main.rs"}` | 0 / 0.326 | `cli/job.rs:Function/run_agent @ 1918; main.rs tests @ 2742, 2774` | exact, 3/3 sites; 2/2 files | `ryi graph --uses Subscribe --root <hafley-rs> <hafley-rs>`; rc=0, `0 edges: 0 +, 0 ~, 0 -` vs 3 SCIP sites. Dry-run `ryi rename crates/boop/src/main.rs#Subscribe Subscribe_zz --root <hafley-rs> --json`; rc=0, 1 site vs 3 SCIP sites. |
| `runs/serena/hafley-rs`; term `top` | `{"name_path":"top","relative_path":"crates/boop-mux/src/_0_snapshot.rs"}` | 0 / 0.542 | `turnstrip/src/_2_place.rs:Function/place_window @ 99; harness/src/pane.rs @ 237,565` | exact, 61/61 sites; 10/10 files | `ryi graph --uses top --root <hafley-rs> <hafley-rs>`; rc=0, `0 edges: 0 +, 0 ~, 0 -` vs 61 SCIP sites. Dry-run `ryi rename crates/boop-mux/src/_0_snapshot.rs#top top_zz --root <hafley-rs> --json`; rc=6, `glob import reaches the symbol at runtime`. |
| `crates/hafley_scm`; fn `type_refs` | `{"name_path":"type_refs","relative_path":"crates/hafley_scm/src/lang/rust/6_type_refs.rs"}` | 0 / 26.622 | `lang/rust/7_type_entity_rows.rs:Function/callable @ 261,272` | exact, 15/15 sites; 4/4 files | `ryi graph --callers type_refs --root <hafley-rs> <hafley-rs>`; rc=0, 11 edges vs 15 sites |
| `crates/hafley_scm`; type `Synthetic` | `{"name_path":"Synthetic","relative_path":"crates/hafley_scm/src/lang/rust/9_type_candidate_rows.rs"}` | 0 / 0.273 | `lang/rust/9_type_candidate_rows.rs:Function/collect @ 98; @ 153; @ 244` | exact, 7/7 sites; 3/3 files | `ryi graph --uses Synthetic --root <hafley-rs> <hafley-rs>`; rc=0, `0 edges: 0 +, 0 ~, 0 -` vs 7 SCIP sites. Dry-run `ryi rename crates/hafley_scm/src/lang/rust/9_type_candidate_rows.rs#Synthetic Synthetic_zz --root <hafley-rs> --json`; rc=0, 4 sites vs 7 SCIP sites. |
| `crates/hafley_scm`; term `region` | `{"name_path":"region","relative_path":"crates/hafley_scm/src/read/lang/4_owned_region.rs"}` | 0 / 0.253 | `read/lang/4_owned_region.rs:Function/propose_owned_region @ 116; Method/changed @ 132` | exact, 5/5 sites; 1/1 file | `ryi graph --uses region --root <hafley-rs> <hafley-rs>`; rc=0, `0 edges: 0 +, 0 ~, 0 -` vs 5 SCIP sites. Dry-run `ryi rename crates/hafley_scm/src/read/lang/4_owned_region.rs#region region_zz --root <hafley-rs> --json`; rc=7, empty output (abstain exit). |
| Bench `tokio`; fn `insert_at` | `{"name_path":"insert_at","relative_path":"tokio-util/src/time/delay_queue.rs"}` | 0 / 27.706 | `tokio-util/tests/panic.rs:Function/delay_queue_insert_at_panic_caller @ 114` | exact, 65/65 sites; 3/3 files | `ryi graph --callers insert_at --root <tokio> <tokio>`; rc=0, 1 edge vs 65 sites |
| Bench `tokio`; type `UnixStream` | `{"name_path":"UnixStream","relative_path":"tokio/src/net/unix/stream.rs"}` | 1 / 0.154 | `Error executing tool find_referencing_symbols: ValueError: No symbol matching 'UnixStream' found` | fail; 0/75 sites, 0/15 files | `ryi graph --uses UnixStream --root <tokio> <tokio>`; rc=0, 9 `graph_decline` records (`external_crate`) vs 75 SCIP sites. Dry-run `ryi rename tokio/src/net/unix/stream.rs#UnixStream UnixStream_zz --root <tokio> --json`; rc=4, `tokio/src/net/unix/stream.rs declares no UnixStream`. |
| Bench `tokio`; term `children` | `{"name_path":"children","relative_path":"tokio-util/src/sync/cancellation_token/tree_node.rs"}` | 0 / 0.165 | `tokio-util/src/sync/cancellation_token/tree_node.rs:Function @ 76; 25 site lines` | exact, 25/25 sites; 1/1 file | `ryi graph --uses children --root <tokio> <tokio>`; rc=0, `0 edges: 0 +, 0 ~, 0 -` vs 25 SCIP sites. Dry-run `ryi rename tokio-util/src/sync/cancellation_token/tree_node.rs#children children_zz --root <tokio> --json`; rc=7, empty output (abstain exit). |

`<hafley-rs>` expands to `/Users/chrishafley/.cache/lanes/claude-375/eval/bench/runs/serena/hafley-rs`; `<tokio>` expands to `/Users/chrishafley/.cache/lanes/claude-375/eval/bench/runs/serena/tokio`. API tests executed: nine calls across the workspace copy, `crates/hafley_scm`, and Tokio. Other endpoint runtime rows remain pending. The HTTP dashboard endpoints remain uncalled under the dashboard-off configuration.

### RYii CLI API inventory

`ryi` and `ryii` expose the same command names and options in their installed binary help. Both binaries' help was read for every listed subcommand. Synopsis and flags follow; `PATH` positional inputs are file, directory, or glob inputs where the help describes them.

| Command | Synopsis | Flags |
|---|---|---|
| capabilities | `ryi capabilities` | `-h`, `--help` |
| fast | `ryi fast [OPTIONS] [PATH]...` | `--pattern`, `--entry`, `--depth`, `--root`, `--sqlite`, `--lines`, `-h`, `--help` |
| slow | `ryi slow [OPTIONS] [PATH]...` | `--pattern`, `--entry`, `--depth`, `--root`, `--sqlite`, `--lines`, `--scip-index`, `--no-checker`, `--scip-timeout`, `-h`, `--help` |
| scip | `ryi scip [OPTIONS] [PATH]...` | `--pattern`, `--entry`, `--depth`, `--root`, `--sqlite`, `--lines`, `--scip-index`, `--scip-cache`, `--scip-timeout`, `--indexer`, `--raw`, `--records`, `--occurrence-text`, `--scip-build`, `-h`, `--help` |
| graph | `ryi graph [OPTIONS] <query> [PATH]...` | `--pattern`, `--entry`, `--depth`, `--root`, `--callers`, `--uses`, `--from`, `--call-path`, `--type-path`, `--flow-path`, `--slice`, `--sqlite`, `--slow`, `--timeout`, `--at`, `--compare`, `--scip-index`, `--rust-checker`, `--ts-checker`, `--go-checker`, `-h`, `--help` |
| cleave | `ryi cleave [OPTIONS] [TARGET] [DEST]` | `--list`, `--root`, `--state`, `--drag`, `--commit`, `--verify`, `--text-refs`, `--json`, `-h`, `--help` |
| move | `ryi move [OPTIONS] [OLD] [NEW]` | `--list`, `--root`, `--verify-cwd`, `--state`, `--commit`, `--shim`, `--relocate-mod`, `--verify`, `--text-refs`, `-h`, `--help` |
| rename | `ryi rename [OPTIONS] [TARGET] [NEW]` | `--list`, `--root`, `--state`, `--at`, `--commit`, `--text-refs`, `--verify-scip`, `--no-scip-merge`, `--json`, `-h`, `--help` |
| query | `ryi query [OPTIONS] --query <QUERY> [PATH]...` | `--pattern`, `--entry`, `--depth`, `--root`, `--lang`, `--query`, `--digest`, `--sqlite`, `-h`, `--help` |
| region | `ryi region [OPTIONS] <TARGET> <ID>` | `--generated`, `--apply`, `--state`, `-h`, `--help` |
| watch | `ryi watch [OPTIONS]` | `--root`, `--pattern`, `--kinds`, `--receipts`, `--once`, `--poll-ms`, `-h`, `--help` |
| diff | `ryi diff [OPTIONS] --from <REV> --to <REV>` | `--root`, `--from`, `--to`, `--pattern`, `--arms`, `--sqlite`, `-h`, `--help` |
| ingest | `ryi ingest [OPTIONS] <PATH>...` | `--sqlite`, `-h`, `--help` |
| schema | `ryi schema` | `-h`, `--help` |
| trail | `ryi trail [N]` | `-h`, `--help` |
| stratify | `ryi stratify [OPTIONS] [PATH]...` | `--pattern`, `--entry`, `--depth`, `--root`, `--from`, `--base-lines`, `--kind`, `-h`, `--help` |

Runtime matrix scenarios are available for `graph --callers`, `graph --uses` and `rename` in the RYii call/result column above; other RYii subcommands remain uncalled. Full help stdout for both binaries is retained at `~/.cache/lanes/claude-375/eval/serena/ryi-help.txt`. The `graph --uses` help states that it returns declarations that reference a type; it is not a general term-reference query. The per-type table records `graph --uses` and, where capability supports it, dry-run `rename` site outputs. Term targets use dry-run rename where supported.

### Serena CLI surface

`serena --help` reports all root commands. The following verbatim help capture contains all recursively listed subcommands, argument forms, and flags from `serena <command> --help` (45 help invocations, all exit code 0).

<details><summary>CLI subcommand help and flags</summary>

```text
===== serena  --help (rc=0) =====
Usage: serena [OPTIONS] COMMAND [ARGS]...

  Main serena CLI commands. Note that you also have access to `serena-hooks`
  CLI commands which are kept under that separate entrypoint for performance
  reasons, see `serena-hooks --help`. You can run `<command> --help` for more
  info on each command.

Options:
  -V, --version  Show the version and exit.
  --help         Show this message and exit.

Commands:
  config                Manage Serena configuration.
  context               Manage Serena contexts.
  dashboard-viewer      Open the Serena dashboard viewer for a given URL.
  init                  Initialize Serena by creating a global config...
  memories              Inspect, read, write, and validate Serena memories.
  mode                  Manage Serena modes.
  print-system-prompt   Print the system prompt for a project.
  project               Manage Serena projects.
  prompts               Commands related to Serena's prompts that are...
  setup                 Set up Serena for use with a specific client by...
  start-mcp-server      Starts the Serena MCP server.
  start-project-server  Starts the Serena project server, which exposes...
  tools                 Commands related to Serena's tools.
===== serena config --help (rc=0) =====
Usage: serena config [OPTIONS] COMMAND [ARGS]...

  Manage Serena configuration.

Options:
  --help  Show this message and exit.

Commands:
  edit  Edit serena_config.yml in your default editor.
===== serena config edit --help (rc=0) =====
Usage: serena config edit [OPTIONS]

  Edit serena_config.yml in your default editor. Will create a config file
  from the template if no config is found.

Options:
  --help  Show this message and exit.
===== serena context --help (rc=0) =====
Usage: serena context [OPTIONS] COMMAND [ARGS]...

  Manage Serena contexts. You can run `context <command> --help` for more info
  on each command.

Options:
  --help  Show this message and exit.

Commands:
  create  Create a new context or copy an internal one.
  delete  Delete a custom context file.
  edit    Edit a custom context YAML file.
  list    List available contexts.
===== serena context create --help (rc=0) =====
Usage: serena context create [OPTIONS]

  Create a new context or copy an internal one.

Options:
  -n, --name TEXT       Name for the new context. If --from-internal is passed
                        may be left empty to create a context of the same
                        name, which will then override the internal context
  --from-internal TEXT  Copy from an internal context.
  --help                Show this message and exit.
===== serena context delete --help (rc=0) =====
Usage: serena context delete [OPTIONS] CONTEXT_NAME

  Delete a custom context file.

Options:
  --help  Show this message and exit.
===== serena context edit --help (rc=0) =====
Usage: serena context edit [OPTIONS] CONTEXT_NAME

  Edit a custom context YAML file.

Options:
  --help  Show this message and exit.
===== serena context list --help (rc=0) =====
Usage: serena context list [OPTIONS]

  List available contexts.

Options:
  --help  Show this message and exit.
===== serena dashboard-viewer --help (rc=0) =====
Usage: serena dashboard-viewer [OPTIONS] URL

  Open the Serena dashboard viewer for a given URL.

Options:
  --width INTEGER   Window width.  [default: 1400]
  --height INTEGER  Window height.  [default: 900]
  --help            Show this message and exit.
===== serena init --help (rc=0) =====
Usage: serena init [OPTIONS]

  Initialize Serena by creating a global config file with the specified
  default language backend.

Options:
  -b, --language-backend [LSP|JetBrains]
                                  Default code intelligence backend (can be
                                  overridden in the project config).
                                  [default: LSP]
  --help                          Show this message and exit.
===== serena memories --help (rc=0) =====
Usage: serena memories [OPTIONS] COMMAND [ARGS]...

  Inspect, read, write, and validate Serena memories. You can run `serena
  memories <command> --help` for more info on each command.

Options:
  --help  Show this message and exit.

Commands:
  auto-prefix-references  Rewrite exact bare occurrences of existing...
  check                   Check referential integrity across all memories...
  delete                  Delete a memory file.
  edit                    Replace content matching a pattern in a memory.
  initialize              Initialize this project's memory layout by...
  list                    List memories of the active project (and global...
  read                    Print the content of a memory to stdout.
  rename                  Rename or move a memory and update every...
  write                   Write a memory file.
===== serena memories auto-prefix-references --help (rc=0) =====
Usage: serena memories auto-prefix-references [OPTIONS] [PROJECT]

  Rewrite exact bare occurrences of existing memory names by adding the `mem:`
  prefix. This is a heuristic, file-mutating operation: a word that happens to
  coincide with a memory name will be rewritten as a reference even if it was
  intended as ordinary prose. Use --dry-run to preview the rewrites without
  modifying any files.

  Scope is narrower than what `serena memories check` reports. Only EXACT bare
  occurrences are rewritten (the body text must equal an existing memory name
  verbatim); fuzzy near-miss findings surfaced by `check` are NOT autofixable
  here, since rewriting them would require substring substitution rather than
  a prefix addition — they are reported for manual review. By default the
  rewrite is further restricted to memory names containing `/` or longer than
  the configured threshold, and skips global and read-only memories; use the
  --include-* flags below to widen the scope.

Options:
  --dry-run             Preview the rewrites this command would apply; do not
                        modify any files.
  --include-flat-names  Also rewrite short, flat memory names (no `/`, below
                        the length threshold). Raises false-positive risk
                        significantly.
  --include-read-only   Also rewrite occurrences inside read-only memories.
  --include-global      Also rewrite occurrences inside global memories
                        (affects every project consuming them).
  --help                Show this message and exit.
===== serena memories check --help (rc=0) =====
Usage: serena memories check [OPTIONS] [PROJECT]

  Check referential integrity across all memories of the project (and global
  memories). By default reports only stale `mem:` references. Pass --include-
  unmarked to also report bare occurrences of existing memory names (exact
  matches) and --fuzzy-matching (only meaningful in combination with
  --include-unmarked) to additionally report fuzzy near-misses. Read-only and
  never writes. Always exits 0.

Options:
  --include-unmarked  Also report bare exact occurrences of existing memory
                      names (i.e. without the `mem:` prefix).
  --fuzzy-matching    Additionally report fuzzy near-misses (long bare tokens
                      that similarity-match an existing memory name). Only
                      meaningful together with --include-unmarked; ignored
                      otherwise.
  --help              Show this message and exit.
===== serena memories delete --help (rc=0) =====
Usage: serena memories delete [OPTIONS] MEMORY_NAME [PROJECT]

  Delete a memory file. Use the `global/` prefix to address a global memory
  (e.g. `global/style_guide`).

Options:
  --help  Show this message and exit.
===== serena memories edit --help (rc=0) =====
Usage: serena memories edit [OPTIONS] MEMORY_NAME [PROJECT]

  Replace content matching a pattern in a memory. By default operates in
  literal (non-regex) mode and refuses to replace more than one occurrence;
  pass --mode regex to enable regex matching and --allow-multiple-occurrences
  to permit multiple hits.

Options:
  --needle TEXT                 The text to search for (literal by default;
                                regex if --mode=regex).  [required]
  --repl TEXT                   The replacement text (verbatim).  [required]
  --mode [literal|regex]        Treat --needle as literal text or as a Python
                                regex (MULTILINE and DOTALL flags enabled).
                                [default: literal]
  --allow-multiple-occurrences  Permit and apply multiple matches; without
                                this, multiple matches raise an error.
  --help                        Show this message and exit.
===== serena memories initialize --help (rc=0) =====
Usage: serena memories initialize [OPTIONS] [PROJECT]

  Initialize this project's memory layout by seeding the `memory_mainten/ace`
  memory. Requires the project to already exist as a Serena project (run
  `serena project create` first); this command does not create a `.serena`
  directory on its own. If a `global/memory_mainten/ace` memory exists, it
  takes precedence and no project copy is created.

Options:
  --help  Show this message and exit.
===== serena memories list --help (rc=0) =====
Usage: serena memories list [OPTIONS] [PROJECT]

  List memories of the active project (and global memories).

Options:
  -t, --topic TEXT  Restrict the listing to a single topic (e.g. 'auth' or
                    'global/style').
  --help            Show this message and exit.
===== serena memories read --help (rc=0) =====
Usage: serena memories read [OPTIONS] MEMORY_NAME [PROJECT]

  Print the content of a memory to stdout.

Options:
  --help  Show this message and exit.
===== serena memories rename --help (rc=0) =====
Usage: serena memories rename [OPTIONS] OLD_NAME NEW_NAME [PROJECT]

  Rename or move a memory and update every `mem:OLD_NAME` reference across all
  memories. Use `/` in the name to organize into topics; use the `global/`
  prefix to address a global memory. Moving between project and global scope
  is supported.

Options:
  --help  Show this message and exit.
===== serena memories write --help (rc=0) =====
Usage: serena memories write [OPTIONS] MEMORY_NAME [PROJECT]

  Write a memory file. Reads the content from --content, --file, or stdin (in
  that order of precedence).

Options:
  --content TEXT  Memory content provided directly on the command line.
  --file FILE     Read memory content from the given file.
  --help          Show this message and exit.
===== serena mode --help (rc=0) =====
Usage: serena mode [OPTIONS] COMMAND [ARGS]...

  Manage Serena modes. You can run `mode <command> --help` for more info on
  each command.

Options:
  --help  Show this message and exit.

Commands:
  create  Create a new mode or copy an internal one.
  delete  Delete a custom mode file.
  edit    Edit a custom mode YAML file.
  list    List available modes.
===== serena mode create --help (rc=0) =====
Usage: serena mode create [OPTIONS]

  Create a new mode or copy an internal one.

Options:
  -n, --name TEXT       Name for the new mode. If --from-internal is passed
                        may be left empty to create a mode of the same name,
                        which will then override the internal mode.
  --from-internal TEXT  Copy from an internal mode.
  --help                Show this message and exit.
===== serena mode delete --help (rc=0) =====
Usage: serena mode delete [OPTIONS] MODE_NAME

  Delete a custom mode file.

Options:
  --help  Show this message and exit.
===== serena mode edit --help (rc=0) =====
Usage: serena mode edit [OPTIONS] MODE_NAME

  Edit a custom mode YAML file.

Options:
  --help  Show this message and exit.
===== serena mode list --help (rc=0) =====
Usage: serena mode list [OPTIONS]

  List available modes.

Options:
  --help  Show this message and exit.
===== serena print-system-prompt --help (rc=0) =====
Usage: serena print-system-prompt [OPTIONS] [PROJECT]

  Print the system prompt for a project.

Options:
  --only-instructions  Print only the initial instructions, without
                       prefix/postfix.
  --context TEXT       Built-in context name or path to custom context YAML.
                       [default: desktop-app]
  --mode TEXT          Built-in mode names or paths to custom mode YAMLs with which to 
                       override the default_modes defined in the global Serena configuration or 
                       the active project.
                       For details on mode configuration, see 
                         https://oraios.github.io/serena/02-usage/050_configuration.html#modes.
  --help               Show this message and exit.
===== serena project --help (rc=0) =====
Usage: serena project [OPTIONS] COMMAND [ARGS]...

  Manage Serena projects. You can run `project <command> --help` for more info
  on each command.

Options:
  --help  Show this message and exit.

Commands:
  create           Create a new Serena project configuration.
  health-check     Perform a comprehensive health check of the project's...
  index            Index a project by saving symbols to the LSP cache.
  index-file       Index a single file by saving its symbols to the LSP...
  is_ignored_path  Check if a path is ignored by the project configuration.
===== serena project create --help (rc=0) =====
Usage: serena project create [OPTIONS] [PROJECT_PATH]

  Create a new Serena project configuration.

Options:
  --name TEXT                     Project name; defaults to directory name if
                                  not specified.
  --ls, --language TEXT           Language server(s); inferred if not
                                  specified. Can be passed multiple times.
  --index                         Index the project after creation.
  --log-level [DEBUG|INFO|WARNING|ERROR|CRITICAL]
                                  Log level for indexing (only used if --index
                                  is set).
  --timeout FLOAT                 Timeout for indexing a single file (only
                                  used if --index is set).
  --help                          Show this message and exit.
===== serena project health-check --help (rc=0) =====
Usage: serena project health-check [OPTIONS] [PROJECT]

  Perform a comprehensive health check of the project's tools and language
  server.

Options:
  --help  Show this message and exit.
===== serena project index --help (rc=0) =====
Usage: serena project index [OPTIONS] [PROJECT]

  Index a project by saving symbols to the LSP cache. Auto-creates project.yml
  if it doesn't exist.

Options:
  --name TEXT                     Project name (only used if auto-creating
                                  project.yml).
  --ls, --language TEXT           Language server(s) (only used if auto-
                                  creating project.yml). Inferred if not
                                  specified.
  --log-level [DEBUG|INFO|WARNING|ERROR|CRITICAL]
                                  Log level for indexing.
  --timeout FLOAT                 Timeout for indexing a single file.
  --help                          Show this message and exit.
===== serena project index-file --help (rc=0) =====
Usage: serena project index-file [OPTIONS] FILE [PROJECT]

  Index a single file by saving its symbols to the LSP cache.

Options:
  -v, --verbose  Print detailed information about the indexed symbols.
  --help         Show this message and exit.
===== serena project is_ignored_path --help (rc=0) =====
Usage: serena project is_ignored_path [OPTIONS] PATH [PROJECT]

  Check if a path is ignored by the project configuration.

Options:
  --help  Show this message and exit.
===== serena prompts --help (rc=0) =====
Usage: serena prompts [OPTIONS] COMMAND [ARGS]...

  Commands related to Serena's prompts that are outside of contexts and modes.

Options:
  --help  Show this message and exit.

Commands:
  create-override                 Create an override of an internal...
  delete-override                 Delete a prompt override file
  edit-override                   Edit an existing prompt override file
  list                            Lists prompt names and YAML files that...
  list-overrides                  List existing prompt override files
  print-cc-system-prompt-override
                                  To be used specifically in Claude Code...
  print-prompt-template           prints the (unrendered) template for...
===== serena prompts create-override --help (rc=0) =====
Usage: serena prompts create-override [OPTIONS] PROMPT_YAML_NAME

  Create an override of an internal prompts yaml for customizing Serena's
  prompts

Options:
  --help  Show this message and exit.
===== serena prompts delete-override --help (rc=0) =====
Usage: serena prompts delete-override [OPTIONS] PROMPT_YAML_NAME

  Delete a prompt override file

Options:
  --help  Show this message and exit.
===== serena prompts edit-override --help (rc=0) =====
Usage: serena prompts edit-override [OPTIONS] PROMPT_YAML_NAME

  Edit an existing prompt override file

Options:
  --help  Show this message and exit.
===== serena prompts list --help (rc=0) =====
Usage: serena prompts list [OPTIONS]

  Lists prompt names and YAML files that can be overridden.

Options:
  --help  Show this message and exit.
===== serena prompts list-overrides --help (rc=0) =====
Usage: serena prompts list-overrides [OPTIONS]

  List existing prompt override files

Options:
  --help  Show this message and exit.
===== serena prompts print-prompt-template --help (rc=0) =====
Usage: serena prompts print-prompt-template [OPTIONS] PROMPT_NAME

  prints the (unrendered) template for the corresponding prompt name. This
  respects custom prompt yaml overrides and thus will print the value that
  will be used in Serena

Options:
  --help  Show this message and exit.
===== serena setup --help (rc=0) =====
Usage: serena setup [OPTIONS] {claude-code|codebuddy|codex|grok}

  Set up Serena for use with a specific client by registering it as an MCP
  server.

Options:
  --help  Show this message and exit.
===== serena start-mcp-server --help (rc=0) =====
Usage: serena start-mcp-server [OPTIONS]

  Starts the Serena MCP server.

Options:
  --project [PROJECT_NAME|PROJECT_PATH]
                                  Path or name of project to activate at
                                  startup.
  --project-file [PROJECT_NAME|PROJECT_PATH]
                                  [DEPRECATED] Use --project instead.
  --context TEXT                  Built-in context name or path to custom
                                  context YAML.  [default: desktop-app]
  --mode TEXT                     Built-in mode names or paths to custom mode YAMLs with which to 
                                  override the default_modes defined in the global Serena configuration or 
                                  the active project.
                                  For details on mode configuration, see 
                                    https://oraios.github.io/serena/02-usage/050_configuration.html#modes.
  --add-mode TEXT                 Mode names or paths to custom mode YAMLs which shall
                                  be added on top of the other modes specified by the global/project configuration.
                                  For details on mode configuration, see 
                                    https://oraios.github.io/serena/02-usage/050_configuration.html#modes.
  --language-backend [LSP|JetBrains]
                                  Override the configured language backend.
  --transport [stdio|sse|streamable-http]
                                  Transport protocol.  [default: stdio]
  --host TEXT                     Listen address for the MCP server (when
                                  using corresponding transport).  [default:
                                  127.0.0.1]
  --port INTEGER                  Listen port for the MCP server (when using
                                  corresponding transport).  [default: 8000]
  --enable-web-dashboard BOOLEAN  Enable the web dashboard (overriding the
                                  setting in Serena's config). It is
                                  recommended to always enable the dashboard.
                                  If you don't want the browser to open on
                                  startup, set open-web-dashboard to False.
                                  For more information, see https://oraios.git
                                  hub.io/serena/02-usage/060_dashboard.html
  --enable-gui-log-window BOOLEAN
                                  Enable the gui log window (currently only
                                  displays logs; overriding the setting in
                                  Serena's config).
  --open-web-dashboard BOOLEAN    Open Serena's dashboard in your browser
                                  after MCP server startup (overriding the
                                  setting in Serena's config).
  --log-level [DEBUG|INFO|WARNING|ERROR|CRITICAL]
                                  Override log level in config.
  --trace-lsp-communication BOOLEAN
                                  Whether to trace LSP communication.
  --tool-timeout FLOAT            Override tool execution timeout in config.
  --project-from-cwd              Auto-detect project from current working
                                  directory (nearest ancestor containing
                                  .serena/project.yml or .git). If none is
                                  found, no project is activated. Intended for
                                  CLI-based agents like Claude Code, Gemini
                                  and Codex.
  --help                          Show this message and exit.
===== serena start-project-server --help (rc=0) =====
Usage: serena start-project-server [OPTIONS]

  Starts the Serena project server, which exposes project querying
  capabilities via HTTP.

Options:
  --host TEXT                     Listen address for the project server.
                                  [default: 127.0.0.1]
  --port INTEGER                  Listen port for the project server (default:
                                  ProjectServer.PORT).
  --log-level [DEBUG|INFO|WARNING|ERROR|CRITICAL]
                                  Override log level in config.
  --help                          Show this message and exit.
===== serena tools --help (rc=0) =====
Usage: serena tools [OPTIONS] COMMAND [ARGS]...

  Commands related to Serena's tools. You can run `serena tools <command>
  --help` for more info on each command.

Options:
  --help  Show this message and exit.

Commands:
  description  Print the description of a tool, optionally with a...
  list         Prints an overview of the tools that are active by default...
===== serena tools description --help (rc=0) =====
Usage: serena tools description [OPTIONS] TOOL_NAME

  Print the description of a tool, optionally with a specific context (the
  latter may modify the default description).

Options:
  --context TEXT  Context name or path to context file.
  --help          Show this message and exit.
===== serena tools list --help (rc=0) =====
Usage: serena tools list [OPTIONS]

  Prints an overview of the tools that are active by default (not just the
  active ones for your project). For viewing all tools, pass `--all / -a`

Options:
  -q, --quiet
  -a, --all        List all tools, including those not enabled by default.
  --only-optional  List only optional tools (those not enabled by default).
  --help           Show this message and exit.
```
</details>

## Rename scenarios

| Case | Language | Serena result | RYii result | Serena wall | RYii wall |
|---|---|---|---|---:|---:|
| hafley-rs `flatten_type` references (reference API cross-check) | Rust | one reference in `flatten_each` at `crates/hafley_scm/src/read/wire.rs:109`; exact response in server log `mcp_20260928-013734_14230.txt` | pending | 63.0 s total, 36.65 s startup + 27.29 s call | pending |
| hafley_scm detached fixture first pass | Rust | fail: rust-analyzer `cargo metadata` failed because path dependency `hafley-observe` was missing; exact error in log `mcp_20260928-013615_9017.txt`; retried using the full workspace copy | n/a | 9.0 s | n/a |

The reference result is checked against the source call at `crates/hafley_scm/src/read/wire.rs:109` and the SCIP truth target `flatten_type` (`src/read/wire.rs`, line 298, `refs=2` in `truth.db`). `ryi graph --callers flatten_type --root <hafley-rs-copy> <hafley-rs-copy>` remains to be recorded.

## Reference score

File and site precision/recall are micro-aggregated from per-target score rows. Rust rows compare two clients of rust-analyzer against SCIP generated from rust-analyzer and are labeled `same engine as truth`. Python and TypeScript rows are independent comparisons: Pyright vs scip-python, and tsserver vs scip-typescript. Serena’s displayed Rust reference lines are zero-based; scoring converts to SCIP’s one-based line numbers.

| Repo | Kind | Targets | File precision | File recall | Site precision | Site recall | Status |
|---|---|---:|---:|---:|---:|---:|---|
| codegraph-src | fn | 13 | 1.000 | 1.000 | 1.000 | 1.000 | independent; queried |
| codegraph-src | type | 8 | 1.000 | 1.000 | 0.908 | 1.000 | independent; queried |
| codegraph-src | term | 8 | 1.000 | 0.878 | 1.000 | 0.494 | independent; queried |
| codegraph-src | other | 3 | nan | 0.000 | nan | 0.000 | independent; queried |
| django | fn | 13 | 0.974 | 1.000 | 0.997 | 1.000 | independent; queried |
| django | type | 8 | 0.989 | 1.000 | 0.983 | 1.000 | independent; queried |
| django | term | 11 | 1.000 | 1.000 | 0.995 | 1.000 | independent; queried |
| django | other | pending | pending | pending | pending | pending | pending |
| gin | fn | 11 | 1.000 | 1.000 | 1.000 | 1.000 | independent; queried |
| gin | type | 9 | 1.000 | 1.000 | 1.000 | 1.000 | independent; queried |
| gin | term | 9 | 1.000 | 1.000 | 1.000 | 1.000 | independent; queried |
| gin | other | pending | pending | pending | pending | pending | pending |
| graphify-src | fn | 14 | 1.000 | 0.239 | 1.000 | 0.394 | independent; queried |
| graphify-src | type | 9 | 1.000 | 1.000 | 1.000 | 1.000 | independent; queried |
| graphify-src | term | 9 | 1.000 | 0.969 | 1.000 | 0.851 | independent; queried |
| graphify-src | other | pending | pending | pending | pending | pending | pending |
| hafley-rs | fn | 13 | 1.000 | 1.000 | 1.000 | 1.000 | same engine as truth (rust-analyzer) |
| hafley-rs | type | 8 | 1.000 | 1.000 | 0.959 | 0.984 | same engine as truth (rust-analyzer) |
| hafley-rs | term | 8 | 1.000 | 0.973 | 1.000 | 0.929 | same engine as truth (rust-analyzer) |
| hafley-rs | other | 3 | nan | 0.000 | nan | 0.000 | same engine as truth (rust-analyzer) |
| hafley_scm | fn | 14 | 1.000 | 0.964 | 1.000 | 0.890 | same engine as truth (rust-analyzer) |
| hafley_scm | type | 11 | 1.000 | 1.000 | 0.872 | 0.982 | same engine as truth (rust-analyzer) |
| hafley_scm | term | 7 | 1.000 | 1.000 | 1.000 | 1.000 | same engine as truth (rust-analyzer) |
| hafley_scm | other | pending | pending | pending | pending | pending | same engine as truth (rust-analyzer) |
| requests | fn | 12 | 1.000 | 1.000 | 0.982 | 0.831 | independent; queried |
| requests | type | 9 | 1.000 | 1.000 | 0.989 | 1.000 | independent; queried |
| requests | term | 8 | 1.000 | 1.000 | 0.967 | 1.000 | independent; queried |
| requests | other | pending | pending | pending | pending | pending | pending |
| tokio | fn | 12 | 0.918 | 0.987 | 0.971 | 0.997 | same engine as truth (rust-analyzer) |
| tokio | type | 9 | 1.000 | 0.568 | 0.991 | 0.544 | same engine as truth (rust-analyzer) |
| tokio | term | 6 | 1.000 | 0.714 | 0.978 | 0.918 | same engine as truth (rust-analyzer) |
| tokio | other | 4 | nan | 0.000 | nan | 0.000 | same engine as truth (rust-analyzer) |
| vite | fn | 11 | 1.000 | 1.000 | 1.000 | 1.000 | independent; queried |
| vite | type | 4 | 1.000 | 0.750 | 1.000 | 0.800 | independent; queried |
| vite | term | 9 | 1.000 | 0.333 | 1.000 | 0.088 | independent; queried |
| vite | other | 1 | nan | 0.000 | nan | 0.000 | independent; queried |
| ktor | fn | pending | pending | pending | pending | pending | pending |
| ktor | type | pending | pending | pending | pending | pending | pending |
| ktor | term | pending | pending | pending | pending | pending | pending |
| ktor | other | pending | pending | pending | pending | pending | pending |

## RYii reference score

The query endpoint follows `ryi graph --help`: `--callers` returns resolved call edges to a function; `--uses` returns declarations that reference a type. Type and term rename previews are recorded separately as SCIP site-level outputs. For type targets, the table records both `uses` and `rename` where rename is supported by `ryi capabilities`; for term targets, rename previews are used. `other` contains SCIP categories for which the graph commands have no equivalent.

| Repo | Kind | Endpoint | Targets | rc=0 / targets | Mean wall s | File P/R | Site P/R |
|---|---|---|---:|---:|---:|---:|---:|
| codegraph-src | fn | `callers` | 13 | 13/13 | 0.993 | 1.000/0.514 | 1.000/0.524 |
| codegraph-src | type | `uses` | 8 | 8/8 | 0.891 | 1.000/0.455 | 0.455/0.149 |
| codegraph-src | term | `rename` | 8 | 1/8 | 0.120 | 1.000/0.020 | 0.994/0.481 |
| codegraph-src | other | `rename` | 3 | 0/3 | 0.061 | 0.000/0.000 | 0.000/0.000 |
| django | fn | `callers` | 13 | 0/13 | 4.754 | 0.000/0.000 | 0.000/0.000 |
| django | type | `uses` | 8 | 8/8 | 2.879 | 1.000/0.033 | 1.000/0.014 |
| gin | fn | `callers` | 11 | 11/11 | 0.315 | 1.000/1.000 | 1.000/1.000 |
| gin | type | `uses` | 9 | 9/9 | 0.255 | 1.000/0.400 | 0.231/0.011 |
| graphify-src | fn | `callers` | 14 | 0/14 | 1.520 | 0.000/0.000 | 0.000/0.000 |
| graphify-src | type | `uses` | 9 | 9/9 | 1.313 | 1.000/0.348 | 0.684/0.073 |
| hafley-rs | fn | `callers` | 13 | 13/13 | 3.391 | 1.000/0.720 | 1.000/0.533 |
| hafley-rs | type | `uses` | 8 | 8/8 | 3.423 | 1.000/0.660 | 0.661/0.214 |
| hafley-rs | term | `rename` | 8 | 4/8 | 1.172 | 0.800/0.108 | 0.600/0.038 |
| hafley-rs | other | `rename` | 3 | 0/3 | 1.174 | 0.000/0.000 | 0.000/0.000 |
| hafley_scm | fn | `callers` | 14 | 14/14 | 0.632 | 1.000/0.873 | 1.000/0.851 |
| hafley_scm | type | `uses` | 11 | 11/11 | 0.707 | 1.000/0.651 | 0.487/0.147 |
| hafley_scm | term | `rename` | 7 | 1/7 | 1.398 | 0.000/0.000 | 0.000/0.000 |
| requests | fn | `callers` | 12 | 12/12 | 3.519 | 1.000/0.800 | 1.000/0.523 |
| requests | type | `uses` | 9 | 9/9 | 0.186 | 1.000/0.667 | 0.420/0.236 |
| tokio | fn | `callers` | 12 | 12/12 | 1.846 | 0.842/0.203 | 0.875/0.070 |
| tokio | type | `uses` | 9 | 9/9 | 3.306 | 0.850/0.459 | 0.525/0.102 |
| tokio | term | `rename` | 6 | 1/6 | 0.355 | 1.000/0.143 | 0.500/0.020 |
| tokio | other | `rename` | 4 | 0/4 | 0.355 | 0.000/0.000 | 0.000/0.000 |
| vite | fn | `callers` | 11 | 11/11 | 0.656 | 1.000/1.000 | 1.000/0.826 |
| vite | type | `uses` | 4 | 4/4 | 0.679 | 1.000/1.000 | 0.625/0.500 |
| vite | term | `rename` | 9 | 4/9 | 0.075 | 0.800/0.444 | 0.667/0.140 |
| vite | other | `rename` | 1 | 0/1 | 0.063 | 0.000/0.000 | 0.000/0.000 |

## Per-target real-repo reference results

Serena and RYii reference rows use the same target identity and SCIP ground-truth occurrences. Each result cell is `rc / wall seconds / file P/R / site P/R`. RYii `rename` rows are dry-runs without `--commit`; the output edit sites are scored against SCIP. Type targets show both graph `--uses` and dry-run rename. Python/Go RYii type/term rename is unsupported per `ryi capabilities`.

| Repo | Target | Kind | Serena find_referencing_symbols | RYii query | RYii type rename |
|---|---|---|---|---|---|
| codegraph-src | `broadcast` | fn | 0 / 0.183s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.007s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| codegraph-src | `childForFieldName` | fn | 0 / 0.815s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.944s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| codegraph-src | `extractFunction` | fn | 0 / 0.402s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.009s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| codegraph-src | `findIndexedFile` | fn | 0 / 0.254s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.990s / 1.000/1.000 / 1.000/0.636 | rename: n/a |
| codegraph-src | `getChildByField` | fn | 0 / 4.673s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.957s / 1.000/1.000 / 1.000/0.946 | rename: n/a |
| codegraph-src | `getModuleAggregation` | fn | 0 / 0.251s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.998s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| codegraph-src | `guardsBefore` | fn | 0 / 0.230s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.004s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| codegraph-src | `handleLine` | fn | 0 / 0.182s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.994s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| codegraph-src | `isCppConstructorDeclaration` | fn | 0 / 0.885s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.011s / 1.000/1.000 / 1.000/0.667 | rename: n/a |
| codegraph-src | `namedChild` | fn | 0 / 3.415s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.943s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| codegraph-src | `removeMcpEntryAt` | fn | 0 / 0.180s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.991s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| codegraph-src | `setDefaultProjectHint` | fn | 0 / 0.294s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.069s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| codegraph-src | `truncateOutput` | fn | 0 / 0.402s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.991s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| codegraph-src | `CLIFF_MAX0:` | other | 1 / 0.150s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.060s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| codegraph-src | `bold2:` | other | 1 / 0.194s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.063s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| codegraph-src | `nodesMs0:` | other | 1 / 0.172s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.061s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| codegraph-src | `ambientDeclaration` | term | 1 / 0.154s / 0.000/0.000 / 0.000/0.000 | rename: 6 / 0.119s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| codegraph-src | `extractor` | term | 1 / 0.154s / 0.000/0.000 / 0.000/0.000 | rename: 0 / 0.296s / 1.000/1.000 / 0.994/1.000 | rename: n/a |
| codegraph-src | `isNamespace` | term | 0 / 0.378s / 1.000/1.000 / 1.000/1.000 | rename: 6 / 0.123s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| codegraph-src | `paramsField` | term | 0 / 0.289s / 1.000/1.000 / 1.000/1.000 | rename: 6 / 0.120s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| codegraph-src | `row` | term | 0 / 1.326s / 1.000/1.000 / 1.000/1.000 | rename: 6 / 0.122s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| codegraph-src | `typeLiteral22:deferTools` | term | 1 / 0.156s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.062s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| codegraph-src | `typeLiteral35:intraFileCalls` | term | 1 / 0.149s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.059s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| codegraph-src | `typeLiteral7:files` | term | 1 / 0.147s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.059s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| codegraph-src | `ElidedSymbolRef` | type | 0 / 0.299s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.886s / 1.000/1.000 / 0.000/0.000 | rename: 0 / 0.067s / 1.000/1.000 / 0.900/1.000 |
| codegraph-src | `ExtractionResult` | type | 0 / 0.587s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.896s / 1.000/0.938 / 0.500/0.241 | rename: 0 / 0.176s / 0.941/1.000 / 0.983/1.000 |
| codegraph-src | `LRUCache` | type | 0 / 0.332s / 1.000/1.000 / 0.393/1.000 | uses: 0 / 0.892s / 1.000/0.500 / 0.000/0.000 | rename: 0 / 0.063s / 0.667/1.000 / 0.379/1.000 |
| codegraph-src | `LanguageExtractor` | type | 0 / 0.357s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.899s / 1.000/0.097 / 0.333/0.043 | rename: 0 / 0.061s / 0.969/1.000 / 0.986/1.000 |
| codegraph-src | `ModuleLinkTotal` | type | 0 / 0.253s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.895s / 1.000/1.000 / 0.000/0.000 | rename: 0 / 0.063s / 1.000/1.000 / 0.667/1.000 |
| codegraph-src | `StdioTransportOptions` | type | 0 / 0.153s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.889s / 1.000/1.000 / 0.500/0.500 | rename: 0 / 0.063s / 1.000/1.000 / 0.667/1.000 |
| codegraph-src | `SyntaxTokenClass` | type | 0 / 0.214s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.885s / 1.000/1.000 / 0.750/0.462 | rename: 0 / 0.124s / 1.000/1.000 / 0.929/1.000 |
| codegraph-src | `WireArm` | type | 0 / 0.211s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.889s / 1.000/1.000 / 0.500/0.333 | rename: 0 / 0.061s / 1.000/1.000 / 0.750/1.000 |
| django | `_get_col` | fn | 0 / 0.723s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 4.746s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| django | `add_node` | fn | 0 / 0.749s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 4.678s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| django | `base36_to_int` | fn | 0 / 0.576s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 4.746s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| django | `cache_page` | fn | 0 / 0.776s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 5.071s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| django | `ct_field_attname` | fn | 0 / 4.706s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 4.733s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| django | `fromstr` | fn | 0 / 0.941s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 4.787s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| django | `inlineformset_factory` | fn | 0 / 0.839s / 0.833/1.000 / 0.987/1.000 | callers: 1 / 4.717s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| django | `make_model_tuple` | fn | 0 / 0.609s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 4.730s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| django | `popen_wrapper` | fn | 0 / 0.615s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 4.809s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| django | `remove_model` | fn | 0 / 0.652s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 4.676s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| django | `remove_non_capturing_groups` | fn | 0 / 0.564s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 4.618s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| django | `set_name_with_model` | fn | 0 / 0.607s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 4.744s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| django | `to_asgi_names` | fn | 0 / 0.610s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 4.747s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| django | `CHANGE_FORM` | term | 0 / 1.142s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| django | `E026` | term | 0 / 0.603s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| django | `LOOKUP_SEP` | term | 0 / 1.078s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| django | `MISSING_DESCRIPTION` | term | 0 / 0.607s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| django | `_url_module_exception` | term | 0 / 0.618s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| django | `cs_getsize` | term | 0 / 0.568s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| django | `get_dataset_driver` | term | 0 / 0.564s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| django | `mailers` | term | 0 / 0.750s / 1.000/1.000 / 0.981/1.000 | rename: unsupported / no run | rename: n/a |
| django | `sql_pk_constraint` | term | 0 / 0.616s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| django | `user_logged_out` | term | 0 / 0.638s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| django | `validate_slug` | term | 0 / 0.728s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| django | `ActionLocation` | type | 0 / 0.790s / 1.000/1.000 / 0.970/1.000 | uses: 0 / 2.866s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| django | `AutocompleteMixin` | type | 0 / 0.619s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 2.871s / 1.000/1.000 / 1.000/0.667 | rename: unsupported |
| django | `BaseMonthArchiveView` | type | 0 / 0.643s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 2.926s / 1.000/1.000 / 1.000/1.000 | rename: unsupported |
| django | `Lead` | type | 0 / 0.654s / 0.750/1.000 / 0.818/1.000 | uses: 0 / 2.934s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| django | `LogEntry` | type | 0 / 1.019s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 2.872s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| django | `ObjectDoesNotExist` | type | 0 / 0.763s / 1.000/1.000 / 0.976/1.000 | uses: 0 / 2.887s / 1.000/0.067 / 1.000/0.025 | rename: unsupported |
| django | `OneToOneField` | type | 0 / 1.582s / 1.000/1.000 / 0.992/1.000 | uses: 0 / 2.854s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| django | `PathSerializer` | type | 0 / 0.618s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 2.822s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| gin | `StringToBytes` | fn | 0 / 0.191s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.327s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| gin | `debugPrintWARNINGNew` | fn | 0 / 0.135s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.263s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| gin | `filterFlags` | fn | 0 / 0.142s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.315s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| gin | `getReadHeaderTimeout` | fn | 0 / 0.146s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.324s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| gin | `getTyped` | fn | 0 / 0.282s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.312s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| gin | `getValue` | fn | 0 / 0.148s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.330s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| gin | `nameOfFunction` | fn | 0 / 0.132s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.310s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| gin | `requestHeader` | fn | 0 / 0.151s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.325s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| gin | `setArrayOfMultipartFormFiles` | fn | 0 / 0.123s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.319s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| gin | `trySetCustom` | fn | 0 / 3.631s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.367s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| gin | `trySetUsingParser` | fn | 0 / 0.118s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.276s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| gin | `EnableDecoderUseNumber` | term | 0 / 0.159s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| gin | `anyMethods` | term | 0 / 0.127s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| gin | `children` | term | 0 / 0.195s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| gin | `defaultMemory` | term | 0 / 0.122s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| gin | `defaultPlatform` | term | 0 / 0.117s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| gin | `ginMode` | term | 0 / 0.122s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| gin | `priority` | term | 0 / 0.185s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| gin | `trustedProxies` | term | 0 / 0.120s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| gin | `writermem` | term | 0 / 0.216s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| gin | `BindingUri` | type | 0 / 0.129s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.249s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| gin | `Context` | type | 0 / 2.325s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.247s / 1.000/0.333 / 1.000/0.017 | rename: unsupported |
| gin | `HandlersChain` | type | 0 / 0.191s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.249s / 1.000/0.800 / 0.000/0.000 | rename: unsupported |
| gin | `IRoutes` | type | 0 / 0.192s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.248s / 1.000/0.333 / 0.000/0.000 | rename: unsupported |
| gin | `LoggerConfig` | type | 0 / 0.128s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.248s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| gin | `jsonApi` | type | 0 / 0.126s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.252s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| gin | `msgpackBinding` | type | 0 / 0.144s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.243s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| gin | `nodeType` | type | 0 / 0.174s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.307s / 1.000/1.000 / 0.000/0.000 | rename: unsupported |
| gin | `responseWriter` | type | 0 / 0.230s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.253s / 1.000/0.500 / 0.000/0.000 | rename: unsupported |
| graphify-src | `_extract_parallel` | fn | 0 / 0.318s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 1.468s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `_handle_def_form` | fn | 0 / 0.222s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 1.615s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `_max_server_contexts` | fn | 0 / 0.369s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 1.513s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `_nid` | fn | 0 / 1.018s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 1.520s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `_pascal_strip_comments` | fn | 0 / 0.269s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 1.576s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `_probe_node_link_round_trip` | fn | 0 / 0.232s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 1.502s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `_rebuild_code` | fn | 0 / 5.590s / 1.000/1.000 / 1.000/0.994 | callers: 1 / 1.514s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `_stat_key_to_relative` | fn | 0 / 0.196s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 1.462s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `extract` | fn | 0 / 4.068s / 0.000/0.000 / 0.000/0.000 | callers: 1 / 1.559s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `extract_csharp` | fn | 0 / 0.470s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 1.550s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `extract_verilog` | fn | 0 / 0.559s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 1.541s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `file_hash` | fn | 0 / 0.743s / 1.000/1.000 / 1.000/0.988 | callers: 1 / 1.446s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `resolve_seed` | fn | 0 / 2.276s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 1.590s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `safe_fetch` | fn | 0 / 0.273s / 1.000/1.000 / 1.000/1.000 | callers: 1 / 1.418s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| graphify-src | `GRAPHIFY_OUT` | term | 0 / 0.502s / 1.000/0.929 / 1.000/0.400 | rename: unsupported / no run | rename: n/a |
| graphify-src | `PAPER` | term | 0 / 0.330s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| graphify-src | `VALID_CONFIDENCES` | term | 0 / 0.231s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| graphify-src | `_CONFIG_JSON_KEYS` | term | 0 / 0.226s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| graphify-src | `_GEMINI_NUDGE_TEXT` | term | 0 / 0.485s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| graphify-src | `_NAT64_WKP` | term | 0 / 0.200s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| graphify-src | `_stat_index_dirty` | term | 0 / 0.204s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| graphify-src | `call_function_field` | term | 0 / 0.716s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| graphify-src | `ts_module` | term | 0 / 2.969s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| graphify-src | `FileSlice` | type | 0 / 0.611s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 1.229s / 1.000/0.143 / 1.000/0.038 | rename: unsupported |
| graphify-src | `FileType` | type | 0 / 0.794s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 1.262s / 1.000/0.333 / 0.667/0.029 | rename: unsupported |
| graphify-src | `ImportedSymbol` | type | 0 / 0.259s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 1.323s / 1.000/1.000 / 0.667/0.333 | rename: unsupported |
| graphify-src | `LanguageConfig` | type | 0 / 0.698s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 1.339s / 1.000/0.500 / 0.667/0.087 | rename: unsupported |
| graphify-src | `MinHashLSH` | type | 0 / 0.275s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 1.387s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| graphify-src | `PRInfo` | type | 0 / 0.273s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 1.294s / 1.000/1.000 / 0.625/0.357 | rename: unsupported |
| graphify-src | `ToolError` | type | 0 / 0.380s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 1.342s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| graphify-src | `_NoFileRedirectHandler` | type | 0 / 0.163s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 1.335s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| graphify-src | `_SSRFGuardedHTTPSHandler` | type | 0 / 0.154s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 1.310s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| hafley-rs | `append_row` | fn | 0 / 0.617s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.404s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| hafley-rs | `blob_matches` | fn | 0 / 0.344s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.556s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| hafley-rs | `corpus_defs` | fn | 0 / 0.760s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.351s / 1.000/0.769 / 1.000/0.807 | rename: n/a |
| hafley-rs | `diagnostic_line` | fn | 0 / 3.302s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.391s / 1.000/0.500 / 1.000/0.500 | rename: n/a |
| hafley-rs | `family_span` | fn | 0 / 0.364s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.394s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| hafley-rs | `intern_public` | fn | 0 / 0.480s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.343s / 1.000/0.600 / 1.000/0.164 | rename: n/a |
| hafley-rs | `key_sorted` | fn | 0 / 0.238s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.503s / 1.000/1.000 / 1.000/0.667 | rename: n/a |
| hafley-rs | `kotlin_type_refs` | fn | 0 / 0.263s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.356s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| hafley-rs | `persisted_delta` | fn | 0 / 0.251s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.378s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| hafley-rs | `push_claude_agent` | fn | 0 / 14.289s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.334s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| hafley-rs | `record_status` | fn | 0 / 0.730s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.354s / 1.000/0.300 / 1.000/0.113 | rename: n/a |
| hafley-rs | `reminder_detail` | fn | 0 / 1.937s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.329s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley-rs | `stage_span` | fn | 0 / 0.492s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.396s / 1.000/1.000 / 1.000/0.917 | rename: n/a |
| hafley-rs | `kind_impls!` | other | 1 / 0.231s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 1.180s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley-rs | `skip:` | other | 1 / 0.230s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 1.158s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley-rs | `test:` | other | 1 / 0.206s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 1.185s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley-rs | `BUILD` | term | 0 / 1.481s / 1.000/1.000 / 1.000/1.000 | rename: 0 / 1.141s / 0.500/0.250 / 0.500/0.167 | rename: n/a |
| hafley-rs | `CONTEXT_VIEW_SCHEMA` | term | 0 / 0.400s / 1.000/1.000 / 1.000/1.000 | rename: 0 / 1.228s / 1.000/1.000 / 0.750/1.000 | rename: n/a |
| hafley-rs | `PR_NOTICE_SCHEMA` | term | 0 / 0.256s / 1.000/1.000 / 1.000/1.000 | rename: 0 / 1.167s / 1.000/1.000 / 0.500/1.000 | rename: n/a |
| hafley-rs | `SESSION_GRAPH` | term | 0 / 0.453s / 1.000/0.500 / 1.000/0.154 | rename: 6 / 1.147s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley-rs | `bottom` | term | 0 / 0.491s / 1.000/1.000 / 1.000/1.000 | rename: 6 / 1.150s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley-rs | `main_tree` | term | 0 / 0.389s / 1.000/1.000 / 1.000/1.000 | rename: 7 / 1.191s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley-rs | `output_bytes` | term | 0 / 0.296s / 1.000/1.000 / 1.000/1.000 | rename: 0 / 1.165s / 1.000/1.000 / 0.500/1.000 | rename: n/a |
| hafley-rs | `top` | term | 0 / 0.385s / 1.000/1.000 / 1.000/1.000 | rename: 6 / 1.186s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley-rs | `AgentEvent` | type | 0 / 0.567s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 3.431s / 1.000/1.000 / 0.571/0.235 | rename: 0 / 1.552s / 0.857/1.000 / 0.944/1.000 |
| hafley-rs | `CfgNodeKind` | type | 0 / 0.422s / 1.000/1.000 / 1.000/0.969 | uses: 0 / 3.406s / 1.000/0.667 / 0.667/0.062 | rename: 0 / 1.367s / 1.000/1.000 / 0.970/1.000 |
| hafley-rs | `CpgProperty` | type | 0 / 0.505s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 3.381s / 1.000/0.667 / 0.500/0.250 | rename: 0 / 1.242s / 1.000/1.000 / 0.889/1.000 |
| hafley-rs | `DeferredAdapter` | type | 0 / 0.281s / 1.000/1.000 / 0.167/0.500 | uses: 0 / 3.432s / 1.000/1.000 / 0.500/0.500 | rename: 0 / 1.246s / 1.000/1.000 / 0.667/1.000 |
| hafley-rs | `FamilyMask` | type | 0 / 0.506s / 1.000/1.000 / 0.957/0.985 | uses: 0 / 3.497s / 1.000/0.833 / 0.933/0.412 | rename: 0 / 1.766s / 1.000/1.000 / 0.986/1.000 |
| hafley-rs | `RevisionId` | type | 0 / 0.492s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 3.388s / 1.000/0.467 / 0.250/0.067 | rename: 0 / 1.309s / 1.000/1.000 / 0.984/1.000 |
| hafley-rs | `Subscribe` | type | 0 / 0.258s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 3.463s / 0.000/0.000 / 0.000/0.000 | rename: 0 / 1.299s / 1.000/0.500 / 0.000/0.000 |
| hafley-rs | `SymbolRow` | type | 0 / 0.233s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 3.386s / 0.000/0.000 / 0.000/0.000 | rename: 0 / 1.293s / 0.500/0.500 / 0.500/0.500 |
| hafley_scm | `corpus_defs` | fn | 0 / 0.688s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.634s / 1.000/0.769 / 1.000/0.807 | rename: n/a |
| hafley_scm | `df_push_node` | fn | 0 / 0.361s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.562s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| hafley_scm | `flatten_scip_records` | fn | 0 / 0.228s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.595s / 1.000/0.667 / 1.000/0.667 | rename: n/a |
| hafley_scm | `flatten_type` | fn | 0 / 0.226s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.623s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| hafley_scm | `generic_where_candidates` | fn | 0 / 12.866s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.682s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| hafley_scm | `io_path` | fn | 0 / 0.663s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.611s / 1.000/0.938 / 1.000/0.967 | rename: n/a |
| hafley_scm | `latest_bind` | fn | 0 / 0.249s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.630s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| hafley_scm | `load_type_params` | fn | 0 / 0.221s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.638s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| hafley_scm | `match_limit_check` | fn | 0 / 0.261s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.643s / 1.000/1.000 / 1.000/0.500 | rename: n/a |
| hafley_scm | `node_span` | fn | 0 / 0.495s / 1.000/0.800 / 1.000/0.674 | callers: 0 / 0.637s / 1.000/1.000 / 1.000/0.779 | rename: n/a |
| hafley_scm | `py_first_identifier` | fn | 0 / 0.224s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.663s / 1.000/1.000 / 1.000/0.333 | rename: n/a |
| hafley_scm | `syntax_tsi_rows` | fn | 0 / 0.342s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.644s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| hafley_scm | `type_refs` | fn | 0 / 0.283s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.647s / 1.000/0.500 / 1.000/0.733 | rename: n/a |
| hafley_scm | `walk_data_refs` | fn | 0 / 0.237s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.634s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| hafley_scm | `DEFINITION` | term | 0 / 0.268s / 1.000/1.000 / 1.000/1.000 | rename: 0 / 1.266s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley_scm | `arms` | term | 0 / 0.296s / 1.000/1.000 / 1.000/1.000 | rename: 7 / 1.319s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley_scm | `decorated` | term | 0 / 0.253s / 1.000/1.000 / 1.000/1.000 | rename: 6 / 1.235s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley_scm | `direct` | term | 0 / 0.260s / 1.000/1.000 / 1.000/1.000 | rename: 6 / 1.252s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley_scm | `indexes` | term | 0 / 1.070s / 1.000/1.000 / 1.000/1.000 | rename: 7 / 1.965s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley_scm | `region` | term | 0 / 0.214s / 1.000/1.000 / 1.000/1.000 | rename: 7 / 1.501s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley_scm | `stop` | term | 0 / 0.288s / 1.000/1.000 / 1.000/1.000 | rename: 6 / 1.251s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| hafley_scm | `Always` | type | 0 / 0.225s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.665s / 0.000/0.000 / 0.000/0.000 | rename: 0 / 1.248s / 0.000/0.000 / 0.000/0.000 |
| hafley_scm | `CSharp` | type | 0 / 0.278s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.667s / 0.000/0.000 / 0.000/0.000 | rename: 0 / 1.241s / 0.000/0.000 / 0.000/0.000 |
| hafley_scm | `EvalType` | type | 0 / 0.211s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.734s / 0.000/0.000 / 0.000/0.000 | rename: 0 / 1.212s / 0.000/0.000 / 0.000/0.000 |
| hafley_scm | `ExpandedCallDefRow` | type | 0 / 0.359s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.736s / 1.000/1.000 / 0.000/0.000 | rename: 0 / 1.199s / 0.000/0.000 / 0.000/0.000 |
| hafley_scm | `FlatFact` | type | 0 / 1.239s / 1.000/1.000 / 1.000/0.996 | uses: 0 / 0.668s / 1.000/0.867 / 0.541/0.170 | rename: 0 / 2.644s / 0.000/0.000 / 0.000/0.000 |
| hafley_scm | `Frame` | type | 0 / 0.221s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.749s / 1.000/1.000 / 0.000/0.000 | rename: 0 / 1.204s / 0.000/0.000 / 0.000/0.000 |
| hafley_scm | `GoModuleIndex` | type | 0 / 0.280s / 1.000/1.000 / 1.000/0.909 | uses: 0 / 0.699s / 1.000/0.750 / 0.143/0.091 | rename: 0 / 1.308s / 0.000/0.000 / 0.000/0.000 |
| hafley_scm | `GoRun` | type | 0 / 0.208s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.727s / 0.000/0.000 / 0.000/0.000 | rename: 0 / 1.242s / 0.000/0.000 / 0.000/0.000 |
| hafley_scm | `Policy` | type | 0 / 0.275s / 1.000/1.000 / 0.791/0.971 | uses: 0 / 0.737s / 1.000/0.500 / 0.286/0.057 | rename: 0 / 1.297s / 0.000/0.000 / 0.000/0.000 |
| hafley_scm | `RyiLang` | type | 0 / 0.442s / 1.000/1.000 / 0.525/0.930 | uses: 0 / 0.666s / 1.000/0.692 / 0.529/0.158 | rename: 0 / 1.417s / 0.000/0.000 / 0.000/0.000 |
| hafley_scm | `Synthetic` | type | 0 / 0.229s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.730s / 0.000/0.000 / 0.000/0.000 | rename: 0 / 1.242s / 0.000/0.000 / 0.000/0.000 |
| requests | `_basic_auth_str` | fn | 0 / 0.216s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.527s / 1.000/1.000 / 1.000/0.667 | rename: n/a |
| requests | `cert_verify` | fn | 0 / 0.124s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.488s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| requests | `cookiejar_from_dict` | fn | 0 / 0.212s / 1.000/1.000 / 0.909/0.909 | callers: 0 / 3.514s / 1.000/1.000 / 1.000/0.545 | rename: n/a |
| requests | `extract_cookies_to_jar` | fn | 0 / 0.131s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.542s / 1.000/1.000 / 1.000/0.667 | rename: n/a |
| requests | `guess_json_utf` | fn | 0 / 0.227s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.509s / 1.000/1.000 / 1.000/0.500 | rename: n/a |
| requests | `is_prepared` | fn | 0 / 0.143s / 1.000/1.000 / 1.000/0.231 | callers: 0 / 3.544s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| requests | `prepare_content_length` | fn | 0 / 0.246s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.541s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| requests | `prepare_method` | fn | 0 / 0.121s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.543s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| requests | `select_proxy` | fn | 0 / 0.125s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.501s / 1.000/1.000 / 1.000/0.750 | rename: n/a |
| requests | `sha256_utf8` | fn | 0 / 0.119s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.527s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| requests | `to_native_string` | fn | 0 / 2.272s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.505s / 1.000/0.800 / 1.000/0.615 | rename: n/a |
| requests | `unquote_header_value` | fn | 0 / 0.123s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 3.492s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| requests | `CertType` | term | 0 / 0.135s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| requests | `RawDataType` | term | 0 / 0.117s / 1.000/1.000 / 0.667/1.000 | rename: unsupported / no run | rename: n/a |
| requests | `UriType` | term | 0 / 0.162s / 1.000/1.000 / 0.957/1.000 | rename: unsupported / no run | rename: n/a |
| requests | `_null2` | term | 0 / 0.122s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| requests | `is_urllib3_1` | term | 0 / 0.128s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| requests | `preferred_clock` | term | 0 / 0.123s / 1.000/1.000 / 0.667/1.000 | rename: unsupported / no run | rename: n/a |
| requests | `status_code` | term | 0 / 0.155s / 1.000/1.000 / 1.000/1.000 | rename: unsupported / no run | rename: n/a |
| requests | `str` | term | 0 / 0.372s / 1.000/1.000 / 0.978/1.000 | rename: unsupported / no run | rename: n/a |
| requests | `BaseAdapter` | type | 0 / 0.121s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.183s / 1.000/1.000 / 0.750/0.600 | rename: unsupported |
| requests | `CaseInsensitiveDict` | type | 0 / 0.158s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.191s / 1.000/0.667 / 0.429/0.158 | rename: unsupported |
| requests | `DataKwargs` | type | 0 / 0.125s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.190s / 1.000/1.000 / 0.000/0.000 | rename: unsupported |
| requests | `FileModeWarning` | type | 0 / 0.130s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.184s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| requests | `InvalidJSONError` | type | 0 / 0.125s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.180s / 1.000/0.500 / 1.000/0.250 | rename: unsupported |
| requests | `InvalidProxyURL` | type | 0 / 0.122s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.190s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| requests | `LookupDict` | type | 0 / 0.122s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.182s / 0.000/0.000 / 0.000/0.000 | rename: unsupported |
| requests | `Response` | type | 0 / 0.184s / 1.000/1.000 / 0.979/1.000 | uses: 0 / 0.195s / 1.000/0.800 / 0.394/0.277 | rename: unsupported |
| requests | `_ValidatedRequest` | type | 0 / 0.119s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.181s / 1.000/1.000 / 1.000/1.000 | rename: unsupported |
| tokio | `as_u64` | fn | 0 / 0.286s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.824s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `call` | fn | 0 / 14.062s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.850s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| tokio | `can_auto_advance` | fn | 1 / 0.150s / 0.000/0.000 / 0.000/0.000 | callers: 0 / 1.857s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `changed` | fn | 0 / 0.717s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.878s / 1.000/0.222 / 1.000/0.049 | rename: n/a |
| tokio | `clear_wakers` | fn | 0 / 0.140s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.841s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `enable_all` | fn | 0 / 1.179s / 0.811/1.000 / 0.857/1.000 | callers: 0 / 1.866s / 1.000/0.267 / 1.000/0.167 | rename: n/a |
| tokio | `insert_at` | fn | 0 / 0.611s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.826s / 1.000/0.333 / 1.000/0.015 | rename: n/a |
| tokio | `notified_owned` | fn | 0 / 0.321s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.798s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `set_prev` | fn | 0 / 0.180s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.857s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `simplex` | fn | 0 / 2.300s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.806s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `submit_metrics` | fn | 0 / 0.153s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.863s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| tokio | `unbounded_channel` | fn | 0 / 1.371s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 1.882s / 1.000/0.120 / 1.000/0.037 | rename: n/a |
| tokio | `cfg_net_unix!` | other | 1 / 0.152s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.353s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `cfg_signal_internal!` | other | 1 / 0.150s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.353s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `select!` | other | 1 / 0.146s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.356s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `try_join!` | other | 1 / 0.144s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.358s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `children` | term | 0 / 0.169s / 1.000/1.000 / 1.000/1.000 | rename: 7 / 0.354s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `events` | term | 0 / 0.225s / 1.000/1.000 / 0.667/1.000 | rename: 6 / 0.353s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `node` | term | 0 / 0.164s / 1.000/1.000 / 1.000/1.000 | rename: 7 / 0.352s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `run_queue` | term | 0 / 0.243s / 1.000/1.000 / 1.000/1.000 | rename: 7 / 0.360s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `thread_cap` | term | 0 / 0.165s / 1.000/1.000 / 1.000/1.000 | rename: 0 / 0.356s / 1.000/1.000 / 0.500/0.500 | rename: n/a |
| tokio | `written` | term | 1 / 0.142s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.353s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| tokio | `CancellationToken` | type | 0 / 0.610s / 1.000/1.000 / 0.987/0.915 | uses: 0 / 3.334s / 1.000/0.667 / 0.667/0.171 | rename: 0 / 0.425s / 1.000/1.000 / 0.988/1.000 |
| tokio | `Database` | type | 0 / 0.165s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 3.273s / 1.000/1.000 / 0.500/0.500 | rename: 0 / 0.470s / 1.000/1.000 / 0.667/1.000 |
| tokio | `GlobalOrphanQueue` | type | 0 / 0.254s / 1.000/1.000 / 1.000/0.700 | uses: 0 / 3.334s / 1.000/0.500 / 0.000/0.000 | rename: 0 / 0.359s / 1.000/1.000 / 0.909/1.000 |
| tokio | `PointersInner` | type | 0 / 0.143s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 3.394s / 1.000/1.000 / 0.000/0.000 | rename: 0 / 0.348s / 1.000/1.000 / 0.667/1.000 |
| tokio | `Signal` | type | 0 / 0.230s / 1.000/1.000 / 1.000/0.778 | uses: 0 / 3.344s / 1.000/0.667 / 0.500/0.222 | rename: 0 / 0.357s / 1.000/1.000 / 0.900/1.000 |
| tokio | `TimeHandle` | type | 1 / 0.149s / 0.000/0.000 / 0.000/0.000 | uses: 0 / 3.277s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.358s / 0.000/0.000 / 0.000/0.000 |
| tokio | `TimerHandle` | type | 0 / 0.196s / 1.000/1.000 / 1.000/0.917 | uses: 0 / 3.279s / 1.000/0.750 / 0.571/0.333 | rename: 0 / 0.420s / 1.000/0.500 / 0.857/0.500 |
| tokio | `TryCurrentErrorKind` | type | 0 / 0.167s / 1.000/1.000 / 1.000/0.727 | uses: 0 / 3.345s / 1.000/1.000 / 0.000/0.000 | rename: 0 / 0.354s / 1.000/1.000 / 0.900/0.818 |
| tokio | `UnixStream` | type | 1 / 0.152s / 0.000/0.000 / 0.000/0.000 | uses: 0 / 3.171s / 0.400/0.133 / 0.000/0.000 | rename: 4 / 0.402s / 0.000/0.000 / 0.000/0.000 |
| vite | `copyDir` | fn | 0 / 0.288s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.646s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| vite | `crash` | fn | 0 / 0.234s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.657s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| vite | `esmifyPostcssLoadConfigDts` | fn | 0 / 0.261s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.651s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| vite | `formatTargetDir` | fn | 0 / 0.259s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.655s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| vite | `parseBundledDependenciesFromLicense` | fn | 0 / 0.496s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.712s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| vite | `readPackageInfo` | fn | 0 / 0.243s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.653s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| vite | `start` | fn | 0 / 0.243s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.651s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| vite | `svgVirtualModulePlugin` | fn | 0 / 0.259s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.638s / 1.000/1.000 / 1.000/0.500 | rename: n/a |
| vite | `testLightningcssVisitorDuringMinify` | fn | 0 / 0.818s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.658s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| vite | `transformFooWithInlineSourceMap` | fn | 0 / 0.247s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.647s / 1.000/1.000 / 1.000/0.500 | rename: n/a |
| vite | `wrapIIFEBabelPlugin` | fn | 0 / 0.294s / 1.000/1.000 / 1.000/1.000 | callers: 0 / 0.648s / 1.000/1.000 / 1.000/1.000 | rename: n/a |
| vite | `decrement0:` | other | 1 / 0.236s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.063s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| vite | `absoluteDepPath` | term | 0 / 0.648s / 1.000/1.000 / 1.000/1.000 | rename: 0 / 0.059s / 1.000/1.000 / 0.500/1.000 | rename: n/a |
| vite | `createModernChunkLegacyGuard` | term | 0 / 0.234s / 0.000/0.000 / 0.000/0.000 | rename: 0 / 0.180s / 0.500/1.000 / 0.750/1.000 | rename: n/a |
| vite | `root` | term | 0 / 0.242s / 1.000/1.000 / 1.000/1.000 | rename: 0 / 0.063s / 1.000/1.000 / 0.667/1.000 | rename: n/a |
| vite | `src` | term | 1 / 0.247s / 0.000/0.000 / 0.000/0.000 | rename: 6 / 0.063s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| vite | `svgVirtualModuleId` | term | 0 / 0.236s / 1.000/1.000 / 1.000/1.000 | rename: 0 / 0.063s / 1.000/1.000 / 0.667/1.000 | rename: n/a |
| vite | `typeLiteral17:id` | term | 1 / 0.226s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.060s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| vite | `typeLiteral22:color` | term | 1 / 0.226s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.063s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| vite | `typeLiteral22:display` | term | 1 / 0.231s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.062s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| vite | `typeLiteral23:link` | term | 1 / 0.226s / 0.000/0.000 / 0.000/0.000 | rename: 4 / 0.063s / 0.000/0.000 / 0.000/0.000 | rename: n/a |
| vite | `MainTypeOnlyClass` | type | 0 / 0.235s / 0.000/0.000 / 0.000/0.000 | uses: 0 / 0.704s / 1.000/1.000 / 0.000/0.000 | rename: 0 / 0.058s / 0.500/1.000 / 0.667/1.000 |
| vite | `PackageJson` | type | 0 / 0.408s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.655s / 1.000/1.000 / 0.500/0.500 | rename: 0 / 0.061s / 1.000/1.000 / 0.800/1.000 |
| vite | `Post` | type | 0 / 0.236s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.649s / 1.000/1.000 / 1.000/0.667 | rename: 0 / 0.061s / 1.000/1.000 / 0.750/1.000 |
| vite | `ShimOptions` | type | 0 / 0.396s / 1.000/1.000 / 1.000/1.000 | uses: 0 / 0.710s / 1.000/1.000 / 1.000/1.000 | rename: 0 / 0.060s / 1.000/1.000 / 0.500/1.000 |

### Rename correctness status

Per-target rename correctness across fresh copies, build/type checks, full diff-site verification, and Serena rename rows remain pending. The RYii rename columns above are dry-run site queries on the indexed copy and do not constitute full rename correctness.

| Repo | Target count | Serena rename | ryi rename | Edited sites vs SCIP | Diff-only-sites | Language check |
|---|---:|---|---|---|---|---|
| codegraph-src | 32 | pending | target preview rows recorded | pending | pending | pending |
| django | 32 | pending | unsupported | pending | pending | pending |
| gin | 29 | pending | unsupported | pending | pending | pending |
| graphify-src | 32 | pending | unsupported | pending | pending | pending |
| hafley-rs | 32 | pending | target preview rows recorded | pending | pending | pending |
| hafley_scm | 32 | pending | target preview rows recorded | pending | pending | pending |
| requests | 29 | pending | unsupported | pending | pending | pending |
| tokio | 31 | pending | target preview rows recorded | pending | pending | pending |
| vite | 25 | pending | target preview rows recorded | pending | pending | pending |
| ktor | pending | pending | pending | pending | pending | pending |


## Tool cost

Cold start is elapsed time from launching the stdio server through MCP `initialize`; warm per-call is the mean `find_referencing_symbols` tool-call wall time across the per-target batch. Peak RSS is the maximum process-tree sum sampled every 50 ms by `bench/run.py` for the persistent batch process. Values are grouped by server language across the listed repos.

| Language server | Cold start s (n repos) | Warm per-call ms (n calls) | Peak process-tree RSS MiB | Observation |
|---|---:|---:|---:|---|
| Rust / rust-analyzer | 1.214 (3) | 856 (95) | 2,988.0 | `hafley-rs`, `hafley_scm`, `tokio` |
| TypeScript / tsserver | 1.375 (2) | 448 (57) | 1,842.6 | `codegraph-src`, `vite` |
| Python / Pyright | 1.408 (3) | 648 (93) | 1,500.0 | `django`, `graphify-src`, `requests` |
| Go / gopls | 1.566 (1) | 352 (29) | 1,133.5 | `gin` |
| Kotlin / Kotlin LS | pending | pending | pending | JDK 21.0.12.1 set from `eval/kotlin-jdk`; server process reports its `intellij-server` build expired. |


## Reproduction commands and MCP calls

Serena installation: `uv venv --python 3.13 ~/.cache/lanes/claude-375/eval/serena/.venv`; `uv pip install --python ~/.cache/lanes/claude-375/eval/serena/.venv/bin/python serena-agent mcp psutil`. Serena version: `1.7.0`. `SERENA_HOME`, UV/Coursier/Go/Gradle caches are redirected under `~/.cache/lanes/claude-375/eval/serena/`. Global telemetry opt-out is absent from the installed Serena config schema; `DO_NOT_TRACK=1` was set. Dashboard disabled with `--enable-web-dashboard False --open-web-dashboard False`.

MCP server command: `serena start-mcp-server --project <copy> --context claude-code --language-backend LSP --enable-web-dashboard False --open-web-dashboard False` over stdio.

Reference call: `tools/call` name `find_referencing_symbols`, arguments `{"name_path":"flatten_type","relative_path":"crates/hafley_scm/src/read/wire.rs"}`. Rename call shape from MCP schema: `tools/call` name `rename_symbol`, arguments `{"name_path":"<symbol>","relative_path":"<definition path>","new_name":"<new name>"}`.

Source/manual pages read: Serena README, Serena usage/configuration docs, Tools and APIs, Running Serena, Dashboard, language support and Rust/TypeScript/Python/Go/Kotlin setup notes; `serena tools list --all`; recursively queried binary CLI `--help`; `ryi`/`ryii --help` and all 16 subcommand help pages, `ryi capabilities`.

## Errors and setup records

| Stage | Exact result | Retry |
|---|---|---|
| Serena `UnixStream` symbol lookup | `ValueError: No symbol matching 'UnixStream' found` | Retried `find_symbol` with `substring_matching:true`, `include_kinds:[23]` (returned `[]`), then reference calls with `UnixStream`, `UnixStream[0]`, `net::unix::stream::UnixStream`, and `tokio::net::unix::stream::UnixStream`; no symbol found. `include_kinds:["Struct"]` failed validation: `Input should be a valid integer, unable to parse string`. Serena docs/source say `include_kinds` is integer LSP kinds. |
| Truth target expansion at `hafley_scm` | Missing `/Users/chrishafley/.cache/lanes/claude-375/eval/repos/hafley_scm/index.scip`; `ryii scip` command returned exit status 1. | Applied `truth.py` selection to existing ingested `occ` rows; added 16 targets, total 32. |
| `bench/run.py` using system Python | `ModuleNotFoundError: No module named 'psutil'` | Retried with eval venv Python. |
| Rust refs on detached `hafley_scm` copy | `cargo metadata` failed: dependency `hafley-observe` path did not exist in copied crate root. | Reran from the full `hafley-rs` workspace at `crates/hafley_scm/src/read/wire.rs`; returned one reference. |
| scip-java initial launch | `Cannot find default main class. Specify one with -M or --main-class.` | Direct cached launcher `.../eval/kotlin-jdk/scip-java index --help` works with JDK 21; ran index attempts against Ktor, Okio, and Coil. |
| scip-java Ktor index | `Could not determine the dependencies of task ':build-settings-logic:jar'.` followed by `Failed to query the value of property 'freeCompilerArgs'.` / `Querying the mapped value ... before task ':build-settings-logic:generatePrecompiledScriptPluginAccessors' has completed is not supported` | Command: `scip-java index --output=<eval>/repos/ktor/index.scip`; Gradle wrapper 9.7.1. No index created. |
| scip-java Okio index | `Could not determine the dependencies of task ':okio-nodefilesystem:scipCompileAll'.` / `Task with path 'compileKotlinJvm' not found in project ':okio-nodefilesystem'.` | Command: `scip-java index --output=<eval>/repos/okio/index.scip`; Gradle wrapper 8.14.3. No index created. |
| scip-java Coil index | `Plugin org.scip_code.scip_java.kotlinc.AnalyzerRegistrar is incompatible with the current version of the compiler.` | Command: `scip-java index --output=<eval>/repos/coil/index.scip`; Gradle wrapper 9.8.0. No index created. |
| Serena Kotlin LSP startup | `This build of intellij-server has expired.`; `LanguageServerTerminatedException: Language server stdout read process terminated unexpectedly` | Set `JAVA_HOME=<eval>/kotlin-jdk/jdk-21.0.12.1+1/Contents/Home` and prepended `$JAVA_HOME/bin` to `PATH`; Ktor `find_referencing_symbols` call could not initialize. Startup failed after 8.905 seconds. Log: `eval/serena/home/logs/2026-09-28/mcp_20260928-094155_52215.txt`. |
| Go SCIP first attempt | `no scip indexer: one index means one indexer, but the paths span ["data", "go", "markdown"]` | Retried with `--indexer go`; local `scip-go` install succeeded; gin `index.scip` created with `scip-go index --skip-tests --output <eval>/repos/gin/index.scip ./...`. SCIP rows and targets not ingested. |


### RYii retry records

| Initial observation | Exact output | Manual/capability check and retry |
|---|---|---|
| `ryi graph --callers` on Django Python targets | `ryi: connection closed before message completed` | Retried per target through the documented CLI shape with a 120-second timeout; all 13 function targets returned exit 1 with that exact stderr. `ryi capabilities` lists Python call resolution; the same targets were queried with Serena/Pyright. |
| `ryi graph --uses` returned empty edge sets | `0 edges: 0 +, 0 ~, 0 -` | Read `ryi graph --help`: `--uses <NAME>` means declarations referencing type NAME. These were type targets, not caller/term queries. Compared each target against SCIP; added dry-run `ryi rename FILE#NAME NAME_zz --root ROOT --json` for all Rust/TypeScript types and terms. Python/Go rename is marked unsupported by `ryi capabilities`. |
| `ryi rename` for `hafley_scm` targets | Initial calls used the detached crate directory as corpus root and returned exit 2. | Reran with workspace root `runs/serena/hafley-rs` and path `crates/hafley_scm/<SCIP def_path>`; all 18 type/term dry-runs now have result rows. The first-attempt stderr was replaced by the corrected run in `bench.db`. |

## Status

WIP checkpoint. Serena references and scoring cover all target rows in codegraph-src (32), django (32), gin (29), graphify-src (32), hafley-rs (32), hafley_scm (32), requests (29), tokio (31), and vite (25). Rust scores share rust-analyzer with SCIP; Python and TypeScript rows compare Pyright/tsserver against SCIP-Python/TypeScript. Kotlin references and SCIP truth remain pending after three scip-java index attempts and a Serena Kotlin LSP initialization failure; JDK 21 was present and supplied. Nine Serena find_referencing_symbols runtime scenarios cover three symbols each on the workspace copy, crates/hafley_scm, and Tokio. These provide RYii graph and rename comparisons for the same scenarios. RYii target queries cover all 113 function, 75 type, and 65 supported term targets; type rename dry-runs cover Rust/TypeScript targets. Rename correctness and remaining endpoint/cost rows remain pending.
