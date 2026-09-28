# Serena evaluation: rename and references against ryi

## API matrix (installed Serena 1.7.0; binary and MCP)

The first table lists all 29 MCP tool schemas returned by stdio `tools/list` under `--context desktop-app`; these schemas were serialized from the server and preserve parameter types, required fields, and defaults. Optional integrations discovered by `serena tools list --all` but not activated in the LSP context are listed separately. Serena CLI help and dashboard routes follow. This report’s measured repo scope is Rust and TypeScript. Detailed reference scenarios were completed for `find_referencing_symbols`; rename and symbol-validation calls are also counted below. Per-endpoint runtime scenarios for the remaining MCP tools and HTTP routes remain pending.

| surface | endpoint | parameters_and_flags | runtime_scenarios |
|---|---|---|---|
| MCP | `create_text_file` | `relative_path` string required; `content` string required | pending |
| MCP | `replace_content` | `relative_path` string required; `needle` string required; `repl` string required; `mode` string enum(literal,regex) required; `allow_multiple_occurrences` boolean optional default=False | pending |
| MCP | `replace_in_files` | `needle` string required; `repl` string required; `mode` string enum(literal,regex) required; `relative_path` string optional default=''; `paths_include_glob` string optional default=''; `paths_exclude_glob` string optional default=''; `dry_run` boolean optional default=False; `occurrence_ids` array<string>\|null optional default=None; `expected_count` integer optional default=-1; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `replace_symbol_body` | `name_path` string required; `relative_path` string required; `body` string required | pending |
| MCP | `insert_after_symbol` | `name_path` string required; `relative_path` string required; `body` string required | pending |
| MCP | `insert_before_symbol` | `name_path` string required; `relative_path` string required; `body` string required | pending |
| MCP | `read_file` | `relative_path` string required; `start_line` integer optional default=0; `end_line` integer\|null optional default=None; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `list_dir` | `relative_path` string required; `recursive` boolean required; `skip_ignored_files` boolean optional default=False; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `find_file` | `file_mask` string required; `relative_path` string required | pending |
| MCP | `search_for_pattern` | `substring_pattern` string required; `context_lines_before` integer optional default=0; `context_lines_after` integer optional default=0; `paths_include_glob` string optional default=''; `paths_exclude_glob` string optional default=''; `relative_path` string optional default=''; `restrict_search_to_code_files` boolean optional default=False; `skip_ignored_files` boolean optional default=True; `multiline` boolean optional default=True; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `get_symbols_overview` | `relative_path` string required; `depth` integer optional default=-1; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `find_symbol` | `name_path_pattern` string required; `depth` integer optional default=0; `relative_path` string optional default=''; `include_body` boolean optional default=False; `include_info` boolean optional default=False; `include_kinds` array<integer> optional default=[]; `exclude_kinds` array<integer> optional default=[]; `substring_matching` boolean optional default=False; `max_matches` integer optional default=-1; `max_answer_chars` integer optional default=-1 | 31 rename mismatch verification calls |
| MCP | `find_referencing_symbols` | `name_path` string required; `relative_path` string required; `include_kinds` array<integer> optional default=[]; `exclude_kinds` array<integer> optional default=[]; `max_answer_chars` integer optional default=-1 | 152 target calls; 9 detailed runtime rows below |
| MCP | `find_implementations` | `name_path` string required; `relative_path` string required; `include_info` boolean optional default=False; `include_kinds` array<integer> optional default=[]; `exclude_kinds` array<integer> optional default=[]; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `find_declaration` | `relative_path` string required; `regex` string required; `containing_symbol_name_path` string\|null optional default=None; `include_body` boolean optional default=False; `include_info` boolean optional default=False | pending |
| MCP | `get_diagnostics_for_file` | `relative_path` string required; `start_line` integer optional default=0; `end_line` integer optional default=-1; `min_severity` integer optional default=4; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `rename_symbol` | `name_path` string required; `relative_path` string required; `new_name` string required | 152 target renames; 6 corrected-name retries |
| MCP | `safe_delete_symbol` | `name_path_pattern` string required; `relative_path` string required | pending |
| MCP | `write_memory` | `memory_name` string required; `content` string required; `max_chars` integer optional default=-1 | pending |
| MCP | `read_memory` | `memory_name` string required | pending |
| MCP | `list_memories` | `topic` string optional default='' | pending |
| MCP | `delete_memory` | `memory_name` string required | pending |
| MCP | `rename_memory` | `old_name` string required; `new_name` string required | pending |
| MCP | `edit_memory` | `memory_name` string required; `needle` string required; `repl` string required; `mode` string enum(literal,regex) required; `allow_multiple_occurrences` boolean optional default=False | pending |
| MCP | `execute_shell_command` | `command` string required; `cwd` string\|null optional default=None; `capture_stderr` boolean optional default=True; `max_answer_chars` integer optional default=-1 | pending |
| MCP | `activate_project` | `project` string required | pending |
| MCP | `get_current_config` | none | pending |
| MCP | `onboarding` | none | pending |
| MCP | `initial_instructions` | none | pending |

Optional MCP integrations listed by `serena tools list --all` but not activated by the LSP server configuration, so not evaluated as callable endpoints: `delete_lines`, `get_diagnostics_for_symbol`, `insert_at_line`, `jet_brains_debug`, `jet_brains_find_declaration`, `jet_brains_find_implementations`, `jet_brains_find_referencing_symbols`, `jet_brains_find_symbol`, `jet_brains_get_symbols_overview`, `jet_brains_inline_symbol`, `jet_brains_list_inspections`, `jet_brains_move`, `jet_brains_rename`, `jet_brains_safe_delete`, `jet_brains_type_hierarchy`, `list_queryable_projects`, `open_dashboard`, `query_project`, `remove_project`, `replace_lines`, `restart_language_server`, `serena_info`. Their descriptions are in `serena tools list --all`; they have no `tools/list` input schema in the active server context.

### Runtime matrix rows: `find_referencing_symbols`

MCP server command: `serena start-mcp-server --project <project> --context claude-code --language-backend LSP --enable-web-dashboard False --open-web-dashboard False`. Each call used `tools/call` with the listed JSON arguments. Serena’s Rust reference lines are zero-based; comparison normalizes to SCIP one-based lines. “Correct” below refers to measured file/site precision and recall against SCIP.

| Project | Scenario | Exact MCP call | return_code | wall_seconds | Serena output sample | file_precision | file_recall | site_precision | site_recall | Closest ryi call | ryi_return_code | ryi output sample | ryi correctness |
|---|---|---|---:|---:|---|---:|---:|---:|---:|---|---:|---|---|
| `runs/serena/hafley-rs` | fn `push_claude_agent` | `find_referencing_symbols({"name_path":"push_claude_agent","relative_path":"crates/boop-harness/src/harness/claude.rs"})` | 0 | 122.361 | `claude.rs:parse_claude_agent_worktrees @ 888, 898` | 1.000 | 1.000 | 1.000 | 1.000 | `ryi graph --callers push_claude_agent --root <hafley-rs> <hafley-rs>` | 0 | `2 edges at 889,899` | exact sites |
| `runs/serena/hafley-rs` | type `Subscribe` | `find_referencing_symbols({"name_path":"Subscribe","relative_path":"crates/boop/src/main.rs"})` | 0 | 0.326 | `cli/job.rs:run_agent @ 1918; main.rs tests @ 2742, 2774` | 1.000 | 1.000 | 1.000 | 1.000 | `ryi graph --uses Subscribe --root <hafley-rs> <hafley-rs>`; dry-run rename also run | 0 | `graph: 0 edges; rename: 1 site vs 3 SCIP sites` | partial |
| `runs/serena/hafley-rs` | term `top` | `find_referencing_symbols({"name_path":"top","relative_path":"crates/boop-mux/src/_0_snapshot.rs"})` | 0 | 0.542 | `place_window @ 99; pane.rs @ 237,565` | 1.000 | 1.000 | 1.000 | 1.000 | `ryi graph --uses top --root <hafley-rs> <hafley-rs>`; dry-run rename also run | 0 | `graph: 0 edges; rename return code 6, glob import reaches symbol at runtime` | graph: no sites; rename: fail |
| `runs/serena/hafley-rs` (`crates/hafley_scm`) | fn `type_refs` | `find_referencing_symbols({"name_path":"type_refs","relative_path":"crates/hafley_scm/src/lang/rust/6_type_refs.rs"})` | 0 | 26.622 | `7_type_entity_rows.rs:callable @ 261,272` | 1.000 | 1.000 | 1.000 | 1.000 | `ryi graph --callers type_refs --root <hafley-rs> <hafley-rs>` | 0 | `11 edges vs 15 SCIP sites` | partial recall |
| `runs/serena/hafley-rs` (`crates/hafley_scm`) | type `Synthetic` | `find_referencing_symbols({"name_path":"Synthetic","relative_path":"crates/hafley_scm/src/lang/rust/9_type_candidate_rows.rs"})` | 0 | 0.273 | `collect @ 98, 153, 244` | 1.000 | 1.000 | 1.000 | 1.000 | `ryi graph --uses Synthetic --root <hafley-rs> <hafley-rs>`; dry-run rename also run | 0 | `graph: 0 edges; rename: 4 sites vs 7 SCIP sites` | partial |
| `runs/serena/hafley-rs` (`crates/hafley_scm`) | term `region` | `find_referencing_symbols({"name_path":"region","relative_path":"crates/hafley_scm/src/read/lang/4_owned_region.rs"})` | 0 | 0.253 | `propose_owned_region @ 116; changed @ 132` | 1.000 | 1.000 | 1.000 | 1.000 | `ryi graph --uses region --root <hafley-rs> <hafley-rs>`; dry-run rename also run | 0 | `graph: 0 edges; rename return_code 7, empty output` | graph: no sites; rename: abstain |
| `runs/serena/tokio` | fn `insert_at` | `find_referencing_symbols({"name_path":"insert_at","relative_path":"tokio-util/src/time/delay_queue.rs"})` | 0 | 27.706 | `tokio-util/tests/panic.rs:delay_queue_insert_at_panic_caller @ 114` | 1.000 | 1.000 | 1.000 | 1.000 | `ryi graph --callers insert_at --root <tokio> <tokio>` | 0 | `1 edge vs 65 SCIP sites` | partial recall |
| `runs/serena/tokio` | type `UnixStream` | `find_referencing_symbols({"name_path":"UnixStream","relative_path":"tokio/src/net/unix/stream.rs"})` | 1 | 0.154 | `ValueError: No symbol matching 'UnixStream' found` | 0.000 | 0.000 | 0.000 | 0.000 | `ryi graph --uses UnixStream --root <tokio> <tokio>`; dry-run rename also run | 0 | `9 graph_decline records; rename return_code 4, declares no UnixStream` | fail |
| `runs/serena/tokio` | term `children` | `find_referencing_symbols({"name_path":"children","relative_path":"tokio-util/src/sync/cancellation_token/tree_node.rs"})` | 0 | 0.165 | `tree_node.rs:Function @ 76; 25 site lines` | 1.000 | 1.000 | 1.000 | 1.000 | `ryi graph --uses children --root <tokio> <tokio>`; dry-run rename also run | 0 | `graph: 0 edges; rename return_code 7, empty output` | graph: no sites; rename: abstain |

Nine Serena reference calls cover three scenarios on the `hafley-rs` workspace, three in its `crates/hafley_scm` subtree, and three in Tokio. Runtime execution of the other Serena MCP endpoints and dashboard HTTP routes remains pending. The API inventory above lists endpoint schemas and binary flags; a complete per-endpoint, three-scenario runtime matrix was not completed.

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

## Scope and corpus

Only Rust and TypeScript results are included below. Rust target truth is SCIP from rust-analyzer and Serena also drives rust-analyzer; Rust rows are labeled `same engine as truth`. TypeScript is the independent comparison of tsserver against scip-typescript. Corpora are the five Rust/TypeScript repos with targets in `truth.db`: `hafley-rs`, `hafley_scm`, `tokio`, `codegraph-src`, and `vite`. Target sets contain 16 original targets plus additional targets selected from SCIP occurrences using `truth.py`, for 152 targets total.

## Serena reference score

Rows are micro-aggregated from per-target `bench.db` records. File and site measurements use SCIP path/line occurrences. Each metric occupies a separate cell.

| repo | kind | targets | file_precision | file_recall | site_precision | site_recall | comparison |
|---|---|---:|---:|---:|---:|---:|---|
| codegraph-src | fn | 13 | 1.000 | 1.000 | 1.000 | 1.000 | independent (tsserver vs scip-typescript) |
| codegraph-src | type | 8 | 1.000 | 1.000 | 0.908 | 1.000 | independent (tsserver vs scip-typescript) |
| codegraph-src | term | 8 | 1.000 | 0.878 | 1.000 | 0.494 | independent (tsserver vs scip-typescript) |
| codegraph-src | other | 3 | n/a | 0.000 | n/a | 0.000 | independent (tsserver vs scip-typescript) |
| hafley-rs | fn | 13 | 1.000 | 1.000 | 1.000 | 1.000 | same engine as truth (rust-analyzer) |
| hafley-rs | type | 8 | 1.000 | 1.000 | 0.959 | 0.984 | same engine as truth (rust-analyzer) |
| hafley-rs | term | 8 | 1.000 | 0.973 | 1.000 | 0.929 | same engine as truth (rust-analyzer) |
| hafley-rs | other | 3 | n/a | 0.000 | n/a | 0.000 | same engine as truth (rust-analyzer) |
| hafley_scm | fn | 14 | 1.000 | 0.964 | 1.000 | 0.890 | same engine as truth (rust-analyzer) |
| hafley_scm | type | 11 | 1.000 | 1.000 | 0.872 | 0.982 | same engine as truth (rust-analyzer) |
| hafley_scm | term | 7 | 1.000 | 1.000 | 1.000 | 1.000 | same engine as truth (rust-analyzer) |
| tokio | fn | 12 | 0.918 | 0.987 | 0.971 | 0.997 | same engine as truth (rust-analyzer) |
| tokio | type | 9 | 1.000 | 0.568 | 0.991 | 0.544 | same engine as truth (rust-analyzer) |
| tokio | term | 6 | 1.000 | 0.714 | 0.978 | 0.918 | same engine as truth (rust-analyzer) |
| tokio | other | 4 | n/a | 0.000 | n/a | 0.000 | same engine as truth (rust-analyzer) |
| vite | fn | 11 | 1.000 | 1.000 | 1.000 | 1.000 | independent (tsserver vs scip-typescript) |
| vite | type | 4 | 1.000 | 0.750 | 1.000 | 0.800 | independent (tsserver vs scip-typescript) |
| vite | term | 9 | 1.000 | 0.333 | 1.000 | 0.088 | independent (tsserver vs scip-typescript) |
| vite | other | 1 | n/a | 0.000 | n/a | 0.000 | independent (tsserver vs scip-typescript) |

## RYii reference score

Function rows use documented `ryi graph --callers`. Type rows include both documented `ryi graph --uses` and `ryi rename <file>#<name> <name>_zz --root <repo> --json` without `--commit`; term rows use dry-run rename. `graph --uses` is type-reference semantics. Metrics are micro-aggregated; one endpoint per row.

| repo | tool | endpoint | kind | targets | file_precision | file_recall | site_precision | site_recall | successful_calls | mean_wall_seconds |
|---|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| codegraph-src | ryi | callers | fn | 13 | 1.000 | 0.514 | 1.000 | 0.524 | 13 | 0.993 |
| codegraph-src | ryi | uses | type | 8 | 1.000 | 0.455 | 0.455 | 0.149 | 8 | 0.891 |
| codegraph-src | ryi | rename | type | 8 | 0.948 | 1.000 | 0.870 | 1.000 | 8 | 0.085 |
| codegraph-src | ryi | rename | term | 8 | 1.000 | 0.020 | 0.994 | 0.481 | 1 | 0.120 |
| codegraph-src | ryi | rename | other | 3 | n/a | 0.000 | n/a | 0.000 | 0 | 0.061 |
| hafley-rs | ryi | callers | fn | 13 | 1.000 | 0.720 | 1.000 | 0.533 | 13 | 3.622 |
| hafley-rs | ryi | uses | type | 8 | 1.000 | 0.660 | 0.661 | 0.214 | 8 | 3.423 |
| hafley-rs | ryi | rename | type | 8 | 0.960 | 0.960 | 0.959 | 0.979 | 8 | 1.384 |
| hafley-rs | ryi | rename | term | 8 | 0.800 | 0.108 | 0.600 | 0.038 | 4 | 1.172 |
| hafley-rs | ryi | rename | other | 3 | n/a | 0.000 | n/a | 0.000 | 0 | 1.174 |
| hafley_scm | ryi | callers | fn | 14 | 1.000 | 0.873 | 1.000 | 0.851 | 14 | 0.632 |
| hafley_scm | ryi | uses | type | 11 | 1.000 | 0.651 | 0.487 | 0.147 | 11 | 0.707 |
| hafley_scm | ryi | rename | type | 11 | n/a | 0.000 | n/a | 0.000 | 11 | 1.387 |
| hafley_scm | ryi | rename | term | 7 | n/a | 0.000 | n/a | 0.000 | 1 | 1.398 |
| tokio | ryi | callers | fn | 12 | 0.842 | 0.203 | 0.875 | 0.070 | 12 | 1.846 |
| tokio | ryi | uses | type | 9 | 0.850 | 0.459 | 0.525 | 0.102 | 9 | 3.306 |
| tokio | ryi | rename | type | 9 | 1.000 | 0.514 | 0.945 | 0.583 | 7 | 0.388 |
| tokio | ryi | rename | term | 6 | 1.000 | 0.143 | 0.500 | 0.020 | 1 | 0.355 |
| tokio | ryi | rename | other | 4 | n/a | 0.000 | n/a | 0.000 | 0 | 0.355 |
| vite | ryi | callers | fn | 11 | 1.000 | 1.000 | 1.000 | 0.826 | 11 | 0.656 |
| vite | ryi | uses | type | 4 | 1.000 | 1.000 | 0.625 | 0.500 | 4 | 0.679 |
| vite | ryi | rename | type | 4 | 0.800 | 1.000 | 0.714 | 1.000 | 4 | 0.060 |
| vite | ryi | rename | term | 9 | 0.800 | 0.444 | 0.667 | 0.140 | 4 | 0.075 |
| vite | ryi | rename | other | 1 | n/a | 0.000 | n/a | 0.000 | 0 | 0.063 |

## Serena rename correctness

For each target the runner reset a fresh tracked copy, invoked Serena `rename_symbol` to append `_zz`, compared deleted diff lines with SCIP occurrences, verified changed files, and ran the language check. Site precision/recall are exactly 1 only where deleted lines equal all SCIP file/line occurrences and there are no extra deleted lines. `diff_only_sites` also requires touched paths to equal SCIP occurrence paths. `check_rc` is the per-target check exit code.

| repo | kind | targets | exact_site_targets | diff_only_site_targets | check_pass_targets | check_fail_targets | mean_rename_seconds |
|---|---|---:|---:|---:|---:|---:|---:|
| codegraph-src | fn | 13 | 13 | 13 | 0 | 13 | 5.246 |
| codegraph-src | type | 8 | 7 | 7 | 0 | 8 | 2.176 |
| codegraph-src | term | 8 | 5 | 5 | 0 | 8 | 3.471 |
| codegraph-src | other | 3 | 0 | 0 | 0 | 3 | 3.282 |
| hafley-rs | fn | 13 | 13 | 13 | 13 | 0 | 17.513 |
| hafley-rs | type | 8 | 8 | 8 | 8 | 0 | 27.956 |
| hafley-rs | term | 8 | 7 | 7 | 8 | 0 | 12.741 |
| hafley-rs | other | 3 | 0 | 0 | 3 | 0 | 1.752 |
| hafley_scm | fn | 14 | 13 | 13 | 14 | 0 | 14.682 |
| hafley_scm | type | 11 | 11 | 11 | 11 | 0 | 11.363 |
| hafley_scm | term | 7 | 7 | 7 | 7 | 0 | 12.171 |
| tokio | fn | 12 | 10 | 10 | 12 | 0 | 7.322 |
| tokio | type | 9 | 6 | 6 | 9 | 0 | 5.599 |
| tokio | term | 6 | 4 | 4 | 6 | 0 | 6.079 |
| tokio | other | 4 | 0 | 0 | 4 | 0 | 0.695 |
| vite | fn | 11 | 11 | 11 | 0 | 11 | 12.252 |
| vite | type | 4 | 4 | 4 | 0 | 4 | 11.737 |
| vite | term | 9 | 5 | 5 | 0 | 9 | 7.381 |
| vite | other | 1 | 0 | 0 | 0 | 1 | 3.541 |

## Rename results per target

| repo | kind | symbol | definition_path | truth_sites | edited_sites | serena_result | result_detail | exact_sites | diff_only_sites | check_command | check_return_code | check_seconds |
|---|---|---|---|---:|---:|---|---|---|---|---|---:|---:|
| codegraph-src | fn | `isCppConstructorDeclaration` | `src/extraction/languages/c-cpp.ts` | 4 | 4 | exact | `Successfully renamed 'isCppConstructorDeclaration' to 'isCppConstructorDeclaration_zz' (2 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.312 |
| codegraph-src | fn | `getChildByField` | `src/extraction/tree-sitter-helpers.ts` | 411 | 411 | exact | `Successfully renamed 'getChildByField' to 'getChildByField_zz' (23 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.295 |
| codegraph-src | fn | `extractFunction` | `src/extraction/tree-sitter.ts` | 14 | 14 | exact | `Successfully renamed 'extractFunction' to 'extractFunction_zz' (1 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.264 |
| codegraph-src | fn | `guardsBefore` | `src/graph/branch-guards.ts` | 7 | 7 | exact | `Successfully renamed 'guardsBefore' to 'guardsBefore_zz' (1 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.248 |
| codegraph-src | fn | `getModuleAggregation` | `src/index.ts` | 2 | 2 | exact | `Successfully renamed 'getModuleAggregation' to 'getModuleAggregation_zz' (2 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.257 |
| codegraph-src | fn | `removeMcpEntryAt` | `src/installer/targets/opencode.ts` | 3 | 3 | exact | `Successfully renamed 'removeMcpEntryAt' to 'removeMcpEntryAt_zz' (1 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.303 |
| codegraph-src | fn | `setDefaultProjectHint` | `src/mcp/tools.ts` | 4 | 4 | exact | `Successfully renamed 'setDefaultProjectHint' to 'setDefaultProjectHint_zz' (2 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.199 |
| codegraph-src | fn | `truncateOutput` | `src/mcp/tools.ts` | 15 | 15 | exact | `Successfully renamed 'truncateOutput' to 'truncateOutput_zz' (1 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.259 |
| codegraph-src | fn | `handleLine` | `src/mcp/transport.ts` | 3 | 3 | exact | `Successfully renamed 'handleLine' to 'handleLine_zz' (1 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.859 |
| codegraph-src | fn | `broadcast` | `src/ui-server/api/events.ts` | 4 | 4 | exact | `Successfully renamed 'broadcast' to 'broadcast_zz' (1 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.379 |
| codegraph-src | fn | `findIndexedFile` | `src/ui-server/api/source.ts` | 12 | 12 | exact | `Successfully renamed 'findIndexedFile' to 'findIndexedFile_zz' (5 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.748 |
| codegraph-src | fn | `childForFieldName` | `src/web-tree-sitter.d.ts` | 129 | 129 | exact | `Successfully renamed 'childForFieldName' to 'childForFieldName_zz' (16 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.387 |
| codegraph-src | fn | `namedChild` | `src/web-tree-sitter.d.ts` | 247 | 247 | exact | `Successfully renamed 'namedChild' to 'namedChild_zz' (21 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.508 |
| codegraph-src | other | `bold2:` | `src/bin/codegraph.ts` | 21 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'bold2:' found` | false | false | `pnpm exec tsc --noEmit` | 254 | 0.752 |
| codegraph-src | other | `CLIFF_MAX0:` | `src/mcp/tools.ts` | 2 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'CLIFF_MAX0:' found` | false | false | `pnpm exec tsc --noEmit` | 254 | 0.527 |
| codegraph-src | other | `nodesMs0:` | `src/resolution/c-fnptr-synthesizer.ts` | 6 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'nodesMs0:' found` | false | false | `pnpm exec tsc --noEmit` | 254 | 0.452 |
| codegraph-src | term | `paramsField` | `src/extraction/tree-sitter-types.ts` | 29 | 29 | exact | `Successfully renamed 'paramsField' to 'paramsField_zz' (28 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.378 |
| codegraph-src | term | `extractor` | `src/extraction/tree-sitter.ts` | 154 | 154 | exact | `Successfully renamed 'TreeSitterExtractor/extractor' to 'extractor_zz' (1 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.203 |
| codegraph-src | term | `typeLiteral22:deferTools` | `src/installer/targets/copilot-cli.ts` | 2 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral22:deferTools' found` | false | false | `pnpm exec tsc --noEmit` | 254 | 0.298 |
| codegraph-src | term | `ambientDeclaration` | `src/mcp/explore-diagnostics.ts` | 4 | 4 | exact | `Successfully renamed 'ExploreCandidateMeta/ambientDeclaration' to 'ambientDeclaration_zz' (2 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.199 |
| codegraph-src | term | `isNamespace` | `src/resolution/types.ts` | 22 | 22 | exact | `Successfully renamed 'isNamespace' to 'isNamespace_zz' (3 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.355 |
| codegraph-src | term | `typeLiteral35:intraFileCalls` | `src/ui-server/api/filecode.ts` | 4 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral35:intraFileCalls' found` | false | false | `pnpm exec tsc --noEmit` | 254 | 0.288 |
| codegraph-src | term | `typeLiteral7:files` | `src/ui-server/api/map.ts` | 2 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral7:files' found` | false | false | `pnpm exec tsc --noEmit` | 254 | 0.210 |
| codegraph-src | term | `row` | `src/web-tree-sitter.d.ts` | 109 | 109 | exact | `Successfully renamed 'row' to 'row_zz' (15 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.225 |
| codegraph-src | type | `ModuleLinkTotal` | `src/db/queries.ts` | 3 | 3 | exact | `Successfully renamed 'ModuleLinkTotal' to 'ModuleLinkTotal_zz' (1 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.211 |
| codegraph-src | type | `SyntaxTokenClass` | `src/extraction/syntax-tokens.ts` | 14 | 14 | exact | `Successfully renamed 'SyntaxTokenClass' to 'SyntaxTokenClass_zz' (2 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.210 |
| codegraph-src | type | `LanguageExtractor` | `src/extraction/tree-sitter-types.ts` | 71 | 71 | exact | `Successfully renamed 'LanguageExtractor' to 'LanguageExtractor_zz' (32 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.198 |
| codegraph-src | type | `ElidedSymbolRef` | `src/mcp/tools.ts` | 10 | 10 | exact | `Successfully renamed 'ElidedSymbolRef' to 'ElidedSymbolRef_zz' (1 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.205 |
| codegraph-src | type | `StdioTransportOptions` | `src/mcp/transport.ts` | 3 | 3 | exact | `Successfully renamed 'StdioTransportOptions' to 'StdioTransportOptions_zz' (1 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.219 |
| codegraph-src | type | `LRUCache` | `src/resolution/lru-cache.ts` | 12 | 29 | partial | `Successfully renamed 'LRUCache' to 'LRUCache_zz' (3 changes applied)` | false | false | `pnpm exec tsc --noEmit` | 254 | 0.185 |
| codegraph-src | type | `ExtractionResult` | `src/types.ts` | 59 | 59 | exact | `Successfully renamed 'ExtractionResult' to 'ExtractionResult_zz' (17 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.199 |
| codegraph-src | type | `WireArm` | `src/ui-server/api/program.ts` | 4 | 4 | exact | `Successfully renamed 'WireArm' to 'WireArm_zz' (1 changes applied)` | true | true | `pnpm exec tsc --noEmit` | 254 | 0.190 |
| hafley-rs | fn | `push_claude_agent` | `crates/boop-harness/src/harness/claude.rs` | 3 | 3 | exact | `Successfully renamed 'push_claude_agent' to 'push_claude_agent_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 3.140 |
| hafley-rs | fn | `append_row` | `crates/boop-proc/src/supervise.rs` | 12 | 12 | exact | `Successfully renamed 'append_row' to 'append_row_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 4.395 |
| hafley-rs | fn | `reminder_detail` | `crates/boop-store/src/1_reminder.rs` | 6 | 6 | exact | `Successfully renamed 'reminder_detail' to 'reminder_detail_zz' (2 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.781 |
| hafley-rs | fn | `intern_public` | `crates/boop-store/src/ident.rs` | 56 | 56 | exact | `Successfully renamed 'intern_public' to 'intern_public_zz' (6 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.849 |
| hafley-rs | fn | `record_status` | `crates/boop-store/src/ident.rs` | 54 | 54 | exact | `Successfully renamed 'record_status' to 'record_status_zz' (10 changes applied)` | true | true | `cargo check -j 2` | 0 | 6.173 |
| hafley-rs | fn | `diagnostic_line` | `crates/hafley_scm/src/read/0_request_root.rs` | 3 | 3 | exact | `Successfully renamed 'diagnostic_line' to 'diagnostic_line_zz' (3 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.126 |
| hafley-rs | fn | `kotlin_type_refs` | `crates/hafley_scm/src/read/lang/kotlin.rs` | 7 | 7 | exact | `Successfully renamed 'kotlin_type_refs' to 'kotlin_type_refs_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.118 |
| hafley-rs | fn | `family_span` | `crates/hafley_scm/src/read/trace.rs` | 32 | 32 | exact | `Successfully renamed 'family_span' to 'family_span_zz' (12 changes applied)` | true | true | `cargo check -j 2` | 0 | 6.512 |
| hafley-rs | fn | `stage_span` | `crates/hafley_scm/src/read/trace.rs` | 13 | 13 | exact | `Successfully renamed 'stage_span' to 'stage_span_zz' (2 changes applied)` | true | true | `cargo check -j 2` | 0 | 7.666 |
| hafley-rs | fn | `corpus_defs` | `crates/hafley_scm/src/read/types.rs` | 58 | 58 | exact | `Successfully renamed 'corpus_defs' to 'corpus_defs_zz' (14 changes applied)` | true | true | `cargo check -j 2` | 0 | 8.606 |
| hafley-rs | fn | `key_sorted` | `crates/hafley_scm/src/read/wire.rs` | 4 | 4 | exact | `Successfully renamed 'key_sorted' to 'key_sorted_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.604 |
| hafley-rs | fn | `blob_matches` | `crates/soopy/src/_7e_stage_store.rs` | 2 | 2 | exact | `Successfully renamed 'blob_matches' to 'blob_matches_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 7.352 |
| hafley-rs | fn | `persisted_delta` | `crates/sqlite-ext/src/schema.rs` | 3 | 3 | exact | `Successfully renamed 'persisted_delta' to 'persisted_delta_zz' (2 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.495 |
| hafley-rs | other | `kind_impls!` | `crates/boop-store/src/bus.rs` | 3 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'kind_impls!' found` | false | false | `cargo check -j 2` | 0 | 2.479 |
| hafley-rs | other | `skip:` | `crates/hafley-observe-macros/src/lib.rs` | 4 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'skip:' found` | false | false | `cargo check -j 2` | 0 | 2.419 |
| hafley-rs | other | `test:` | `crates/hafley-observe-macros/src/lib.rs` | 50 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'test:' found` | false | false | `cargo check -j 2` | 0 | 2.477 |
| hafley-rs | term | `bottom` | `crates/boop-mux/src/_0_snapshot.rs` | 58 | 58 | exact | `Successfully renamed 'bottom' to 'bottom_zz' (9 changes applied)` | true | true | `cargo check -j 2` | 0 | 6.500 |
| hafley-rs | term | `top` | `crates/boop-mux/src/_0_snapshot.rs` | 62 | 62 | exact | `Successfully renamed 'top' to 'top_zz' (10 changes applied)` | true | true | `cargo check -j 2` | 0 | 6.215 |
| hafley-rs | term | `CONTEXT_VIEW_SCHEMA` | `crates/boop-store/src/ident.rs` | 4 | 4 | exact | `Successfully renamed 'CONTEXT_VIEW_SCHEMA' to 'CONTEXT_VIEW_SCHEMA_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.718 |
| hafley-rs | term | `PR_NOTICE_SCHEMA` | `crates/boop-store/src/ident.rs` | 2 | 2 | exact | `Successfully renamed 'PR_NOTICE_SCHEMA' to 'PR_NOTICE_SCHEMA_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.753 |
| hafley-rs | term | `main_tree` | `crates/boop-store/src/session.rs` | 15 | 15 | exact | `Successfully renamed 'main_tree' to 'main_tree_zz' (9 changes applied)` | true | true | `cargo check -j 2` | 0 | 8.473 |
| hafley-rs | term | `BUILD` | `crates/boop/src/lib.rs` | 7 | 7 | exact | `Successfully renamed 'BUILD' to 'BUILD_zz' (5 changes applied)` | true | true | `cargo check -j 2` | 0 | 3.724 |
| hafley-rs | term | `SESSION_GRAPH` | `crates/redux/examples/5_machine_enum/model.rs` | 14 | 3 | partial | `Successfully renamed 'SESSION_GRAPH' to 'SESSION_GRAPH_zz' (2 changes applied)` | false | false | `cargo check -j 2` | 0 | 3.931 |
| hafley-rs | term | `output_bytes` | `crates/soopy/examples/2_mutation_plan_scale.rs` | 2 | 2 | exact | `Successfully renamed 'output_bytes' to 'output_bytes_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 3.518 |
| hafley-rs | type | `AgentEvent` | `crates/boop-store/src/event.rs` | 18 | 18 | exact | `Successfully renamed 'AgentEvent' to 'AgentEvent_zz' (7 changes applied)` | true | true | `cargo check -j 2` | 0 | 10.954 |
| hafley-rs | type | `Subscribe` | `crates/boop/src/main.rs` | 4 | 4 | exact | `Successfully renamed 'Subscribe' to 'Subscribe_zz' (2 changes applied)` | true | true | `cargo check -j 2` | 0 | 4.742 |
| hafley-rs | type | `CpgProperty` | `crates/hafley_scm/src/read/cpg_types.rs` | 9 | 9 | exact | `Successfully renamed 'CpgProperty' to 'CpgProperty_zz' (3 changes applied)` | true | true | `cargo check -j 2` | 0 | 6.329 |
| hafley-rs | type | `CfgNodeKind` | `crates/hafley_scm/src/read/types.rs` | 33 | 33 | exact | `Successfully renamed 'CfgNodeKind' to 'CfgNodeKind_zz' (3 changes applied)` | true | true | `cargo check -j 2` | 0 | 9.128 |
| hafley-rs | type | `FamilyMask` | `crates/hafley_scm/src/read/types.rs` | 69 | 69 | exact | `Successfully renamed 'FamilyMask' to 'FamilyMask_zz' (18 changes applied)` | true | true | `cargo check -j 2` | 0 | 7.066 |
| hafley-rs | type | `SymbolRow` | `crates/hafley_scm/src/read/types.rs` | 3 | 3 | exact | `Successfully renamed 'SymbolRow' to 'SymbolRow_zz' (3 changes applied)` | true | true | `cargo check -j 2` | 0 | 7.224 |
| hafley-rs | type | `DeferredAdapter` | `crates/redux/examples/_4_machine_macro.rs` | 3 | 3 | exact | `Successfully renamed 'DeferredAdapter' to 'DeferredAdapter_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.190 |
| hafley-rs | type | `RevisionId` | `crates/soopy/src/_0_types.rs` | 61 | 61 | exact | `Successfully renamed 'RevisionId' to 'RevisionId_zz' (15 changes applied)` | true | true | `cargo check -j 2` | 0 | 8.266 |
| hafley_scm | fn | `generic_where_candidates` | `src/lang/rust/18_tree_type_candidate_rows.rs` | 2 | 2 | exact | `Successfully renamed 'generic_where_candidates' to 'generic_where_candidates_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.859 |
| hafley_scm | fn | `type_refs` | `src/lang/rust/6_type_refs.rs` | 16 | 16 | exact | `Successfully renamed 'type_refs' to 'type_refs_zz' (5 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.013 |
| hafley_scm | fn | `match_limit_check` | `src/pipeline/run_over_file_tree/ts_match_limit_check.rs` | 3 | 3 | exact | `Successfully renamed 'match_limit_check' to 'match_limit_check_zz' (2 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.069 |
| hafley_scm | fn | `io_path` | `src/read/0_request_root.rs` | 62 | 62 | exact | `Successfully renamed 'io_path' to 'io_path_zz' (17 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.187 |
| hafley_scm | fn | `walk_data_refs` | `src/read/lang/prolog/_0_source.rs` | 12 | 12 | exact | `Successfully renamed 'walk_data_refs' to 'walk_data_refs_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.212 |
| hafley_scm | fn | `df_push_node` | `src/read/lang/python/_0_source.rs` | 25 | 25 | exact | `Successfully renamed 'df_push_node' to 'df_push_node_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.219 |
| hafley_scm | fn | `latest_bind` | `src/read/lang/python/_0_source.rs` | 7 | 7 | exact | `Successfully renamed 'latest_bind' to 'latest_bind_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.205 |
| hafley_scm | fn | `py_first_identifier` | `src/read/lang/python/_0_source.rs` | 4 | 4 | exact | `Successfully renamed 'py_first_identifier' to 'py_first_identifier_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.426 |
| hafley_scm | fn | `load_type_params` | `src/read/lang/ts_receivers.rs` | 3 | 3 | exact | `Successfully renamed 'load_type_params' to 'load_type_params_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.470 |
| hafley_scm | fn | `syntax_tsi_rows` | `src/read/project.rs` | 2 | 2 | exact | `Successfully renamed 'syntax_tsi_rows' to 'syntax_tsi_rows_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.353 |
| hafley_scm | fn | `flatten_scip_records` | `src/read/scip_rows.rs` | 4 | 4 | exact | `Successfully renamed 'flatten_scip_records' to 'flatten_scip_records_zz' (3 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.275 |
| hafley_scm | fn | `corpus_defs` | `src/read/types.rs` | 58 | 58 | exact | `Successfully renamed 'corpus_defs' to 'corpus_defs_zz' (14 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.527 |
| hafley_scm | fn | `flatten_type` | `src/read/wire.rs` | 2 | 2 | exact | `Successfully renamed 'flatten_type' to 'flatten_type_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.614 |
| hafley_scm | fn | `node_span` | `src/span.rs` | 96 | 65 | partial | `Successfully renamed 'node_span' to 'node_span_zz' (9 changes applied)` | false | false | `cargo check -j 2` | 0 | 4.620 |
| hafley_scm | term | `region` | `src/read/lang/4_owned_region.rs` | 6 | 6 | exact | `Successfully renamed 'region' to 'region_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.459 |
| hafley_scm | term | `direct` | `src/read/lang/6_scm_family.rs` | 3 | 3 | exact | `Successfully renamed 'direct' to 'direct_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.378 |
| hafley_scm | term | `arms` | `src/read/project.rs` | 13 | 13 | exact | `Successfully renamed 'arms' to 'arms_zz' (2 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.464 |
| hafley_scm | term | `DEFINITION` | `src/read/types.rs` | 13 | 13 | exact | `Successfully renamed 'DEFINITION' to 'DEFINITION_zz' (6 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.463 |
| hafley_scm | term | `decorated` | `src/read/types.rs` | 5 | 5 | exact | `Successfully renamed 'decorated' to 'decorated_zz' (2 changes applied)` | true | true | `cargo check -j 2` | 0 | 6.004 |
| hafley_scm | term | `indexes` | `src/read/types.rs` | 104 | 104 | exact | `Successfully renamed 'indexes' to 'indexes_zz' (14 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.406 |
| hafley_scm | term | `stop` | `src/types/predicate.rs` | 3 | 3 | exact | `Successfully renamed 'stop' to 'stop_zz' (3 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.379 |
| hafley_scm | type | `ExpandedCallDefRow` | `src/lang/rust/14_expanded_call_rows.rs` | 4 | 4 | exact | `Successfully renamed 'ExpandedCallDefRow' to 'ExpandedCallDefRow_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.025 |
| hafley_scm | type | `Synthetic` | `src/lang/rust/9_type_candidate_rows.rs` | 8 | 8 | exact | `Successfully renamed 'Synthetic' to 'Synthetic_zz' (3 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.057 |
| hafley_scm | type | `EvalType` | `src/read/cpg_types.rs` | 2 | 2 | exact | `Successfully renamed 'EvalType' to 'EvalType_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.183 |
| hafley_scm | type | `Policy` | `src/read/deps.rs` | 36 | 36 | exact | `Successfully renamed 'Policy' to 'Policy_zz' (2 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.187 |
| hafley_scm | type | `RyiLang` | `src/read/lang/extract_lang.rs` | 58 | 58 | exact | `Successfully renamed 'RyiLang' to 'RyiLang_zz' (13 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.354 |
| hafley_scm | type | `GoModuleIndex` | `src/read/lang/go_modules.rs` | 12 | 12 | exact | `Successfully renamed 'GoModuleIndex' to 'GoModuleIndex_zz' (4 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.840 |
| hafley_scm | type | `Frame` | `src/read/lang/kotlin_receivers.rs` | 6 | 6 | exact | `Successfully renamed 'Frame' to 'Frame_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.458 |
| hafley_scm | type | `Always` | `src/read/scip.rs` | 3 | 3 | exact | `Successfully renamed 'Always' to 'Always_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.171 |
| hafley_scm | type | `GoRun` | `src/read/scip.rs` | 3 | 3 | exact | `Successfully renamed 'GoRun' to 'GoRun_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.047 |
| hafley_scm | type | `CSharp` | `src/read/scip/scip_proto.rs` | 3 | 3 | exact | `Successfully renamed 'CSharp' to 'CSharp_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.039 |
| hafley_scm | type | `FlatFact` | `src/read/types.rs` | 271 | 271 | exact | `Successfully renamed 'FlatFact' to 'FlatFact_zz' (15 changes applied)` | true | true | `cargo check -j 2` | 0 | 5.170 |
| tokio | fn | `call` | `tokio-util/src/sync/reusable_box.rs` | 2 | 2 | exact | `Successfully renamed 'call' to 'call_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 3.837 |
| tokio | fn | `insert_at` | `tokio-util/src/time/delay_queue.rs` | 66 | 66 | exact | `Successfully renamed 'insert_at' to 'insert_at_zz' (3 changes applied)` | true | true | `cargo check -j 2` | 0 | 0.309 |
| tokio | fn | `simplex` | `tokio/src/io/util/mem.rs` | 5 | 5 | exact | `Successfully renamed 'simplex' to 'simplex_zz' (4 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.885 |
| tokio | fn | `enable_all` | `tokio/src/runtime/builder.rs` | 55 | 64 | partial | `Successfully renamed 'enable_all' to 'enable_all_zz' (38 changes applied)` | false | false | `cargo check -j 2` | 0 | 2.677 |
| tokio | fn | `clear_wakers` | `tokio/src/runtime/io/scheduled_io.rs` | 2 | 2 | exact | `Successfully renamed 'clear_wakers' to 'clear_wakers_zz' (2 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.662 |
| tokio | fn | `submit_metrics` | `tokio/src/runtime/scheduler/current_thread/mod.rs` | 6 | 6 | exact | `Successfully renamed 'submit_metrics' to 'submit_metrics_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.700 |
| tokio | fn | `as_u64` | `tokio/src/runtime/task/id.rs` | 5 | 5 | exact | `Successfully renamed 'as_u64' to 'as_u64_zz' (4 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.739 |
| tokio | fn | `unbounded_channel` | `tokio/src/sync/mpsc/unbounded.rs` | 83 | 83 | exact | `Successfully renamed 'unbounded_channel' to 'unbounded_channel_zz' (26 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.697 |
| tokio | fn | `notified_owned` | `tokio/src/sync/notify.rs` | 31 | 31 | exact | `Successfully renamed 'notified_owned' to 'notified_owned_zz' (2 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.743 |
| tokio | fn | `changed` | `tokio/src/sync/watch.rs` | 42 | 42 | exact | `Successfully renamed 'changed' to 'changed_zz' (10 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.710 |
| tokio | fn | `can_auto_advance` | `tokio/src/time/clock.rs` | 2 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'can_auto_advance' found` | false | false | `cargo check -j 2` | 0 | 0.467 |
| tokio | fn | `set_prev` | `tokio/src/util/linked_list.rs` | 13 | 13 | exact | `Successfully renamed 'set_prev' to 'set_prev_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.690 |
| tokio | other | `cfg_net_unix!` | `tokio/src/macros/cfg.rs` | 8 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'cfg_net_unix!' found` | false | false | `cargo check -j 2` | 0 | 0.464 |
| tokio | other | `cfg_signal_internal!` | `tokio/src/macros/cfg.rs` | 2 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'cfg_signal_internal!' found` | false | false | `cargo check -j 2` | 0 | 0.125 |
| tokio | other | `select!` | `tokio/src/macros/select.rs` | 79 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'select!' found` | false | false | `cargo check -j 2` | 0 | 0.122 |
| tokio | other | `try_join!` | `tokio/src/macros/try_join.rs` | 37 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'try_join!' found` | false | false | `cargo check -j 2` | 0 | 0.146 |
| tokio | term | `children` | `tokio-util/src/sync/cancellation_token/tree_node.rs` | 26 | 26 | exact | `Successfully renamed 'children' to 'children_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 0.322 |
| tokio | term | `written` | `tokio/src/io/util/buf_writer.rs` | 5 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'written' found` | false | false | `cargo check -j 2` | 0 | 0.135 |
| tokio | term | `thread_cap` | `tokio/src/runtime/blocking/pool.rs` | 3 | 3 | exact | `Successfully renamed 'thread_cap' to 'thread_cap_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.767 |
| tokio | term | `events` | `tokio/src/runtime/io/driver.rs` | 3 | 4 | partial | `Successfully renamed 'events' to 'events_zz' (1 changes applied)` | false | false | `cargo check -j 2` | 0 | 2.610 |
| tokio | term | `run_queue` | `tokio/src/runtime/scheduler/multi_thread/worker.rs` | 13 | 13 | exact | `Successfully renamed 'run_queue' to 'run_queue_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.674 |
| tokio | term | `node` | `tokio/src/sync/batch_semaphore.rs` | 5 | 5 | exact | `Successfully renamed 'node' to 'node_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.702 |
| tokio | type | `Database` | `examples/tinydb.rs` | 3 | 3 | exact | `Successfully renamed 'Database' to 'Database_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 0.462 |
| tokio | type | `CancellationToken` | `tokio-util/src/sync/cancellation_token.rs` | 83 | 83 | exact | `Successfully renamed 'CancellationToken' to 'CancellationToken_zz' (9 changes applied)` | true | true | `cargo check -j 2` | 0 | 0.293 |
| tokio | type | `UnixStream` | `tokio/src/net/unix/stream.rs` | 76 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'UnixStream' found` | false | false | `cargo check -j 2` | 0 | 0.139 |
| tokio | type | `GlobalOrphanQueue` | `tokio/src/process/unix/mod.rs` | 11 | 11 | exact | `Successfully renamed 'GlobalOrphanQueue' to 'GlobalOrphanQueue_zz' (2 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.636 |
| tokio | type | `TimeHandle` | `tokio/src/runtime/driver.rs` | 4 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'TimeHandle' found` | false | false | `cargo check -j 2` | 0 | 0.460 |
| tokio | type | `TryCurrentErrorKind` | `tokio/src/runtime/handle.rs` | 12 | 10 | partial | `Successfully renamed 'TryCurrentErrorKind' to 'TryCurrentErrorKind_zz' (1 changes applied)` | false | false | `cargo check -j 2` | 0 | 2.511 |
| tokio | type | `TimerHandle` | `tokio/src/runtime/time/entry.rs` | 13 | 13 | exact | `Successfully renamed 'TimerHandle' to 'TimerHandle_zz' (4 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.701 |
| tokio | type | `Signal` | `tokio/src/signal/unix.rs` | 10 | 10 | exact | `Successfully renamed 'Signal' to 'Signal_zz' (3 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.643 |
| tokio | type | `PointersInner` | `tokio/src/util/linked_list.rs` | 3 | 3 | exact | `Successfully renamed 'PointersInner' to 'PointersInner_zz' (1 changes applied)` | true | true | `cargo check -j 2` | 0 | 2.710 |
| vite | fn | `parseBundledDependenciesFromLicense` | `docs/_data/acknowledgements.data.ts` | 2 | 2 | exact | `Successfully renamed 'parseBundledDependenciesFromLicense' to 'parseBundledDependenciesFromLicense_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.426 |
| vite | fn | `readPackageInfo` | `docs/_data/acknowledgements.data.ts` | 6 | 6 | exact | `Successfully renamed 'readPackageInfo' to 'readPackageInfo_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.472 |
| vite | fn | `copyDir` | `packages/create-vite/src/index.ts` | 2 | 2 | exact | `Successfully renamed 'copyDir' to 'copyDir_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.702 |
| vite | fn | `formatTargetDir` | `packages/create-vite/src/index.ts` | 4 | 4 | exact | `Successfully renamed 'formatTargetDir' to 'formatTargetDir_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.429 |
| vite | fn | `start` | `packages/create-vite/src/index.ts` | 2 | 2 | exact | `Successfully renamed 'start' to 'start_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.860 |
| vite | fn | `wrapIIFEBabelPlugin` | `packages/plugin-legacy/src/index.ts` | 2 | 2 | exact | `Successfully renamed 'wrapIIFEBabelPlugin' to 'wrapIIFEBabelPlugin_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.625 |
| vite | fn | `esmifyPostcssLoadConfigDts` | `packages/vite/rolldown.dts.config.ts` | 2 | 2 | exact | `Successfully renamed 'esmifyPostcssLoadConfigDts' to 'esmifyPostcssLoadConfigDts_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.880 |
| vite | fn | `testLightningcssVisitorDuringMinify` | `playground/css-lightningcss/vite.config.js` | 2 | 2 | exact | `Successfully renamed 'testLightningcssVisitorDuringMinify' to 'testLightningcssVisitorDuringMinify_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.951 |
| vite | fn | `svgVirtualModulePlugin` | `playground/fs-serve/root/svgVirtualModulePlugin.ts` | 7 | 7 | exact | `Successfully renamed 'svgVirtualModulePlugin' to 'svgVirtualModulePlugin_zz' (4 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.953 |
| vite | fn | `transformFooWithInlineSourceMap` | `playground/js-sourcemap/foo-with-sourcemap-plugin.ts` | 3 | 3 | exact | `Successfully renamed 'transformFooWithInlineSourceMap' to 'transformFooWithInlineSourceMap_zz' (2 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.484 |
| vite | fn | `crash` | `playground/ssr-html/src/has-error-deep.ts` | 2 | 2 | exact | `Successfully renamed 'crash' to 'crash_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.626 |
| vite | other | `decrement0:` | `playground/devtools/src/counter.ts` | 2 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'decrement0:' found` | false | false | `pnpm typecheck` | 1 | 1.518 |
| vite | term | `typeLiteral22:color` | `packages/create-vite/src/index.ts` | 14 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral22:color' found` | false | false | `pnpm typecheck` | 1 | 1.599 |
| vite | term | `typeLiteral22:display` | `packages/create-vite/src/index.ts` | 14 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral22:display' found` | false | false | `pnpm typecheck` | 1 | 1.848 |
| vite | term | `typeLiteral23:link` | `packages/create-vite/src/index.ts` | 12 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral23:link' found` | false | false | `pnpm typecheck` | 1 | 1.450 |
| vite | term | `createModernChunkLegacyGuard` | `packages/plugin-legacy/src/snippets.ts` | 4 | 4 | exact | `Successfully renamed 'createModernChunkLegacyGuard' to 'createModernChunkLegacyGuard_zz' (2 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.658 |
| vite | term | `src` | `packages/vite/rolldown.config.ts` | 7 | 7 | exact | `Successfully renamed 'ShimOptions/src' to 'src_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.497 |
| vite | term | `typeLiteral17:id` | `packages/vite/rolldown.dts.config.ts` | 7 | 0 | fail | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral17:id' found` | false | false | `pnpm typecheck` | 1 | 1.618 |
| vite | term | `absoluteDepPath` | `playground/base-conflict/vite.config.ts` | 2 | 2 | exact | `Successfully renamed 'absoluteDepPath' to 'absoluteDepPath_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.411 |
| vite | term | `svgVirtualModuleId` | `playground/fs-serve/root/svgVirtualModulePlugin.ts` | 3 | 3 | exact | `Successfully renamed 'svgVirtualModuleId' to 'svgVirtualModuleId_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.659 |
| vite | term | `root` | `playground/lib/vite.multiple-output.config.js` | 3 | 3 | exact | `Successfully renamed 'root' to 'root_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.566 |
| vite | type | `PackageJson` | `docs/_data/acknowledgements.data.ts` | 5 | 5 | exact | `Successfully renamed 'PackageJson' to 'PackageJson_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.460 |
| vite | type | `Post` | `docs/_data/blog.data.ts` | 4 | 4 | exact | `Successfully renamed 'Post' to 'Post_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.596 |
| vite | type | `ShimOptions` | `packages/vite/rolldown.config.ts` | 2 | 2 | exact | `Successfully renamed 'ShimOptions' to 'ShimOptions_zz' (1 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.564 |
| vite | type | `MainTypeOnlyClass` | `playground/tsconfig-json/src/not-used-type.ts` | 3 | 3 | exact | `Successfully renamed 'MainTypeOnlyClass' to 'MainTypeOnlyClass_zz' (2 changes applied)` | true | true | `pnpm typecheck` | 1 | 1.514 |

## Typecheck failures

| repo | targets | check_command | return_code | check_output_sample |
|---|---:|---|---:|---|
| codegraph-src | 32 | `pnpm exec tsc --noEmit` | 254 | `undefined<br> ERR_PNPM_RECURSIVE_EXEC_FIRST_FAIL  Command "tsc" not found<br> WARN  The "workspaces" field in package.json is not supported by pnpm. Create a "pnpm-workspace.yaml" file instead.<br>` |
| vite | 25 | `pnpm typecheck` | 1 | `'any' type.<br>packages/plugin-legacy typecheck: src/index.ts(567,43): error TS7031: Binding element 'chunks' implicitly has an 'any' type.<br>packages/plugin-legacy typecheck: src/index.ts(719,24): error TS7006: Parameter 'html' implicitly has an 'any' type.<br>packages/plugin-legacy typecheck: src/index.ts(719,32): error TS7031: Binding element 'chunk' implicitly has an 'any' type.<br>packages/plugin-legacy typecheck: src/index.ts(719,39): error TS7031: Binding element 'bundle' implicitly has an 'any' type.<br>packages/plugin-legacy typecheck: src/index.ts(877,20): error TS7006: Parameter '_opts' implicitly has an 'any' type.<br>packages/plugin-legacy typecheck: src/index.ts(877,27): error TS7006: Parameter 'bundle' implicitly has an 'any' type.<br>packages/plugin-legacy typecheck: src/index.ts(1010,6): error TS7006: Parameter 'chunk' implicitly has an 'any' type.<br>packages/plugin-legacy typecheck: src/index.ts(1037,8): error TS7006: Parameter 'chunk' implicitly has an 'any' type.<br>packages/plugin-legacy typecheck: src/index.ts(1059,15): error TS7006: Parameter 'id' implicitly has an 'any' type.<br>packages/plugin-legacy typecheck: src/index.ts(1064,10): error TS7006: Parameter 'id' implicitly has an 'any' type.<br>packages/plugin-legacy typecheck: src/index.ts(1079,20): error TS7006: Parameter 'config' implicitly has an 'any' type.<br>packages/plugin-legacy typecheck: src/index.ts(1082,17): error TS7006: Parameter 'code' implicitly has an 'any' type.<br>packages/plugin` |

`codegraph-src` was initially checked with `pnpm exec tsc --noEmit`; it returned 254 with `ERR_PNPM_RECURSIVE_EXEC_FIRST_FAIL Command "tsc" not found`. `vite` was rerun using `pnpm typecheck`; it returned 1 for all 25 targets, with TypeScript errors in `packages/plugin-legacy/src/index.ts` and child exit 137. One earlier `pnpm exec tsc --noEmit` invocation triggered pnpm workspace dependency setup in the evaluation scratch clone. No further install command was run.

## Tool cost

Cold start is elapsed time through MCP initialize. Warm per-call is the mean reference tool-call wall time from the per-target batch. Peak RSS is maximum process-tree RSS sampled by `bench/run.py`.

| language_server | repos | targets | cold_start_seconds | warm_call_milliseconds | peak_process_tree_rss_mib |
|---|---:|---:|---:|---:|---:|
| Rust rust-analyzer | 3 | 95 | 1.214 | 856 | 2988.0 |
| TypeScript tsserver | 2 | 57 | 1.375 | 448 | 1842.6 |

## Exact commands and MCP calls

Serena MCP command: `serena start-mcp-server --project <copy> --context claude-code --language-backend LSP --enable-web-dashboard False --open-web-dashboard False`. Reference call: `tools/call` name `find_referencing_symbols`, arguments `"name_path":"<symbol>","relative_path":"<definition path>"`. Rename call: `tools/call` name `rename_symbol`, arguments `"name_path":"<symbol>","relative_path":"<definition path>","new_name":"<symbol>_zz"`.

RYii function reference: `ryi graph --callers <symbol> --root <repo> <repo>`. Type reference: `ryi graph --uses <symbol> --root <repo> <repo>`. Dry-run type/term sites: `ryi rename <file>#<symbol> <symbol>_zz --root <repo> --json` with no `--commit`. Rust build/type check: `CARGO_TARGET_DIR=~/.cache/lanes/claude-375/target cargo check -j 2`. TypeScript check: `pnpm typecheck`. `ryi graph --help` was read before interpreting `--uses`; all graph type rows are labeled with that documented meaning. Full RYii help: `~/.cache/lanes/claude-375/eval/serena/ryi-help.txt`.

Runtime endpoint tests beyond `find_referencing_symbols` are pending.

## Exact lookup and rename errors

`find_symbol` was run for each target whose initial rename diff did not exactly match SCIP sites. Empty responses are recorded as no symbol found. Five targets with unique definition candidates were retried using the full LSP `name_path`.

| repo | target | exact_error_text |
|---|---|---|
| codegraph-src | `bold2:` | `Error executing tool rename_symbol: ValueError: No symbol matching 'bold2:' found` |
| codegraph-src | `CLIFF_MAX0:` | `Error executing tool rename_symbol: ValueError: No symbol matching 'CLIFF_MAX0:' found` |
| codegraph-src | `nodesMs0:` | `Error executing tool rename_symbol: ValueError: No symbol matching 'nodesMs0:' found` |
| codegraph-src | `typeLiteral22:deferTools` | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral22:deferTools' found` |
| codegraph-src | `typeLiteral35:intraFileCalls` | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral35:intraFileCalls' found` |
| codegraph-src | `typeLiteral7:files` | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral7:files' found` |
| hafley-rs | `kind_impls!` | `Error executing tool rename_symbol: ValueError: No symbol matching 'kind_impls!' found` |
| hafley-rs | `skip:` | `Error executing tool rename_symbol: ValueError: No symbol matching 'skip:' found` |
| hafley-rs | `test:` | `Error executing tool rename_symbol: ValueError: No symbol matching 'test:' found` |
| tokio | `can_auto_advance` | `Error executing tool rename_symbol: ValueError: No symbol matching 'can_auto_advance' found` |
| tokio | `cfg_net_unix!` | `Error executing tool rename_symbol: ValueError: No symbol matching 'cfg_net_unix!' found` |
| tokio | `cfg_signal_internal!` | `Error executing tool rename_symbol: ValueError: No symbol matching 'cfg_signal_internal!' found` |
| tokio | `select!` | `Error executing tool rename_symbol: ValueError: No symbol matching 'select!' found` |
| tokio | `try_join!` | `Error executing tool rename_symbol: ValueError: No symbol matching 'try_join!' found` |
| tokio | `written` | `Error executing tool rename_symbol: ValueError: No symbol matching 'written' found` |
| tokio | `UnixStream` | `Error executing tool rename_symbol: ValueError: No symbol matching 'UnixStream' found` |
| tokio | `TimeHandle` | `Error executing tool rename_symbol: ValueError: No symbol matching 'TimeHandle' found` |
| vite | `decrement0:` | `Error executing tool rename_symbol: ValueError: No symbol matching 'decrement0:' found` |
| vite | `typeLiteral22:color` | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral22:color' found` |
| vite | `typeLiteral22:display` | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral22:display' found` |
| vite | `typeLiteral23:link` | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral23:link' found` |
| vite | `typeLiteral17:id` | `Error executing tool rename_symbol: ValueError: No symbol matching 'typeLiteral17:id' found` |

| tokio | `UnixStream` reference query | `ValueError: No symbol matching 'UnixStream' found` |
| tokio | `UnixStream` argument validation | `Input should be a valid integer, unable to parse string` for `include_kinds:["Struct"]`; integer kind `[23]` returned no match |
| codegraph-src | `tsc` type check | `ERR_PNPM_RECURSIVE_EXEC_FIRST_FAIL Command "tsc" not found` |
| vite | `pnpm typecheck` | TypeScript diagnostics include `TS7006`, `TS7031`, `TS18046`, `TS2339`; child commands returned exit 137 |

## Status

Rust and TypeScript reference queries, dry-run RYii queries, Serena rename runs, SCIP site comparisons, and language checks are recorded for all 152 targets. Rust build checks passed for all 95 Rust targets. TypeScript checks failed for all 57 TypeScript targets with the repo-specific errors above. The API schema/CLI inventory is complete; runtime scenarios for every API endpoint remain pending. No Kotlin, Go, or Python work is included after the scope cut.
