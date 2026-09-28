| tool | endpoint | kind | targets | file_recall_before | file_recall_after | file_precision_before | file_precision_after | miss_class | evidence_path |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| codegraph | callers | fn | 36 | 0.7507 | 0.7922 | 0.9648 | 0.9653 | usage | `~/.cache/lanes/claude-375/eval/bench/out/codegraph-src.codegraph.callers.41c141e6cd8c.out` |
| codegraph | callers | type | 18 | 0.6641 | 0.7116 | 0.9375 | 0.9375 | usage | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.codegraph.callers.cb38dac1535f.out` |
| codegraph | callers | term | 14 | 0.2602 | 0.2602 | 0.7143 | 0.7143 | usage | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.codegraph.callers.37adba9c90f6.out` |
| codegraph | explore | fn | 36 | 0.7400 | 0.7922 | 0.9532 | 0.9653 | usage | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.codegraph.explore.d02b12bb37e6.out` |
| codegraph | explore | type | 18 | 0.5109 | 0.7116 | 0.9833 | 0.9375 | usage | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.codegraph.explore.c60b9b4fc563.out` |
| codegraph | explore | term | 14 | 0.0714 | 0.2602 | 0.3333 | 0.7143 | usage | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.codegraph.explore.1a288a2577df.out` |
| sem | callers | fn | 36 | 0.6446 | 0.6446 | 1.0000 | 1.0000 | tool limitation | `~/.cache/lanes/claude-375/eval/bench/out/tokio.sem.callers.ab07d934d8e0.out` |
| sem | callers | type | 18 | 0.3854 | 0.3854 | 1.0000 | 1.0000 | tool limitation | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.sem.callers.9db5a58b7f31.out` |
| sem | callers | term | 14 | 0.1429 | 0.1429 | 1.0000 | 1.0000 | tool limitation | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.sem.callers.ceaf3ad6e169.out` |
| sem | dependents | fn | 36 | 0.6446 | 0.6446 | 1.0000 | 1.0000 | tool limitation | `~/.cache/lanes/claude-375/eval/bench/out/tokio.sem.dependents_audit.597e1125c833.out` |
| sem | dependents | type | 18 | 0.3854 | 0.3854 | 1.0000 | 1.0000 | tool limitation | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.sem.dependents_audit.24c37d0a884f.out` |
| sem | dependents | term | 14 | 0.1429 | 0.1429 | 1.0000 | 1.0000 | tool limitation | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.sem.dependents_audit.95d32c7ee83c.out` |
| graphify | affected | fn | 36 | 0.5198 | 0.5675 | 0.9565 | 0.9615 | usage | `~/.cache/lanes/claude-375/eval/bench/out/codegraph-src.graphify.affected_id_audit.7258be21c81e.out` |
| graphify | affected | type | 18 | 0.3851 | 0.3851 | 0.9667 | 0.9667 | tool limitation | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.affected_id_audit.301333c6e883.out` |
| graphify | affected | term | 14 | 0.0000 | 0.0000 | 0.0000 | — | tool limitation | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.affected_id_audit.4716c94a0d45.out` |
| graphify | explain | fn | 36 | 0.4790 | 0.5675 | 0.7778 | 0.9615 | usage | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.explain.8f4330c51945.out` |
| graphify | explain | type | 18 | 0.2751 | 0.3851 | 1.0000 | 0.9667 | usage | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.explain.b8328da47845.out` |
| graphify | explain | term | 14 | 0.0000 | 0.0000 | 0.0000 | — | usage | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.explain.8224a8bec38f.out` |
| graphify | query | fn | 36 | 0.4910 | 0.3891 | 0.3597 | 0.9444 | scorer/parser | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.query.d4abf8d14fec.out` |
| graphify | query | type | 18 | 0.3629 | 0.0000 | 0.7500 | 0.0000 | scorer/parser | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.query.43377716f638.out` |
| graphify | query | term | 14 | 0.2296 | 0.0000 | 0.2000 | — | scorer/parser | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.query.fbbce109676f.out` |
| cbm | trace | type | 18 | 0.4374 | 0.6257 | 0.7963 | 1.0000 | usage | `~/.cache/lanes/claude-375/eval/bench/out/codegraph-src.cbm.trace_audit.845932c859ad.out` |
| cbm | trace | term | 14 | 0.5714 | 0.6571 | 1.0000 | 0.9167 | usage | `~/.cache/lanes/claude-375/eval/bench/out/hafley-rs.cbm.trace_audit.f482d7283d82.out` |

| tool | original_endpoint | audit_endpoint | manual_path | source_path | exact_call |
| --- | --- | --- | --- | --- | --- |
| codegraph | callers | callers_audit | `~/.cache/lanes/claude-375/eval/codegraph/src/README.md:540` | `~/.cache/lanes/claude-375/eval/codegraph/src/src/graph/traversal.ts:313` | `codegraph callers NAME --json --limit 1000 --path REPO` |
| codegraph | explore | callers_audit | `~/.cache/lanes/claude-375/eval/codegraph/src/README.md:538` | `~/.cache/lanes/claude-375/eval/codegraph/src/src/bin/codegraph.ts:1319` | `codegraph callers NAME --json --limit 1000 --path REPO` |
| sem | callers | dependents_audit | `~/.cache/lanes/claude-375/eval/sem/src/README.md:287` | `~/.cache/lanes/claude-375/eval/sem/src/crates/sem-cli/src/commands/query.rs:118` | `sem impact NAME --file DEF_PATH --dependents --no-default-excludes --json` |
| sem | dependents | dependents_audit | `~/.cache/lanes/claude-375/eval/bench/audit/sem_impact_help.txt:1` | `~/.cache/lanes/claude-375/eval/sem/src/crates/sem-cli/src/commands/impact.rs:1009` | `sem impact NAME --file DEF_PATH --dependents --no-default-excludes --json` |
| graphify | affected | affected_id_audit | `~/.cache/lanes/claude-375/eval/bench/audit/graphify_help.txt:55` | `~/.cache/lanes/claude-375/eval/graphify/src/graphify/affected.py:138` | `graphify affected NODE_ID --depth 1 --graph GRAPH` |
| graphify | explain | affected_id_audit | `~/.cache/lanes/claude-375/eval/bench/audit/graphify_help.txt:9` | `~/.cache/lanes/claude-375/eval/graphify/src/graphify/affected.py:190` | `graphify affected NODE_ID --depth 1 --graph GRAPH` |
| graphify | query | query_audit | `~/.cache/lanes/claude-375/eval/bench/audit/graphify_help.txt:50` | `~/.cache/lanes/claude-375/eval/bench/score.py:206` | `graphify query QUESTION --budget 10000 --graph GRAPH` |
| graphify | affected | neighbors2_audit | `~/.cache/lanes/claude-375/eval/graphify/src/graphify/serve.py:1912` | `~/.cache/lanes/claude-375/eval/graphify/src/graphify/serve.py:2098` | `get_neighbors(node_id=NODE_ID, token_budget=100000)` |
| cbm | trace | trace_audit | `~/.cache/lanes/claude-375/eval/cbm/src/README.md:802` | `~/.cache/lanes/claude-375/eval/cbm/src/src/mcp/mcp.c:533` | `trace_path(function_name=QUALIFIED_NAME, direction=inbound, depth=1, include_tests=true, edge_types=[CALLS,CALL_REFERENCE,USAGE,READS,WRITES,USES_TYPE,IMPORTS,INHERITS,IMPLEMENTS,OVERRIDE], limit=5000, max_output_tokens=1000000, format=json)` |
| cbm | search | trace_audit | `~/.cache/lanes/claude-375/eval/cbm/src/README.md:682` | `~/.cache/lanes/claude-375/eval/cbm/src/src/mcp/mcp.c:484` | `search_graph(name_pattern=EXACT_NAME, format=json)` |
| codegraph | callers-help | callers_audit | `~/.cache/lanes/claude-375/eval/bench/audit/codegraph_callers_help.txt:1` | `~/.cache/lanes/claude-375/eval/codegraph/src/src/graph/traversal.ts:313` | `codegraph callers --help` |
| codegraph | explore-help | callers_audit | `~/.cache/lanes/claude-375/eval/bench/audit/codegraph_explore_help.txt:1` | `~/.cache/lanes/claude-375/eval/codegraph/src/src/bin/codegraph.ts:1319` | `codegraph explore --help` |
| codegraph | node-help | callers_audit | `~/.cache/lanes/claude-375/eval/bench/audit/codegraph_node_help.txt:1` | `~/.cache/lanes/claude-375/eval/codegraph/src/README.md:538` | `codegraph node --help` |
| codegraph | query-help | callers_audit | `~/.cache/lanes/claude-375/eval/bench/audit/codegraph_query_help.txt:1` | `~/.cache/lanes/claude-375/eval/codegraph/src/README.md:539` | `codegraph query --help` |
| codegraph | impact-help | callers_audit | `~/.cache/lanes/claude-375/eval/bench/audit/codegraph_impact_help.txt:1` | `~/.cache/lanes/claude-375/eval/codegraph/src/README.md:542` | `codegraph impact --help` |
| sem | callers-help | dependents_audit | `~/.cache/lanes/claude-375/eval/bench/audit/sem_callers_help.txt:1` | `~/.cache/lanes/claude-375/eval/sem/src/README.md:287` | `sem callers --help` |
| sem | impact-help | dependents_audit | `~/.cache/lanes/claude-375/eval/bench/audit/sem_impact_help.txt:1` | `~/.cache/lanes/claude-375/eval/sem/src/crates/sem-cli/src/commands/impact.rs:1009` | `sem impact --help` |
| sem | find-help | dependents_audit | `~/.cache/lanes/claude-375/eval/bench/audit/sem_find_help.txt:1` | `~/.cache/lanes/claude-375/eval/sem/src/README.md:282` | `sem find --help` |
| sem | refs-help | dependents_audit | `~/.cache/lanes/claude-375/eval/bench/audit/sem_refs_help.txt:1` | `~/.cache/lanes/claude-375/eval/sem/src/README.md:290` | `sem refs --help` |
| cbm | cli-help | trace_audit | `~/.cache/lanes/claude-375/eval/bench/audit/cbm_cli_help.txt:1` | `~/.cache/lanes/claude-375/eval/cbm/src/README.md:239` | `codebase-memory-mcp cli --help` |
| cbm | search-help | trace_audit | `~/.cache/lanes/claude-375/eval/bench/audit/cbm_search_graph_help.txt:1` | `~/.cache/lanes/claude-375/eval/cbm/src/src/store/store.c:5400` | `codebase-memory-mcp cli search_graph --help` |
| sem | entities-help | dependents_audit | `~/.cache/lanes/claude-375/eval/bench/audit/sem_entities_help.txt:1` | `~/.cache/lanes/claude-375/eval/sem/src/README.md:256` | `sem entities --help` |
| graphify | cli-help | affected_id_audit | `~/.cache/lanes/claude-375/eval/bench/audit/graphify_help.txt:1` | `~/.cache/lanes/claude-375/eval/graphify/src/README.md:513` | `graphify --help` |
| codegraph | tools/list | callers_audit | `~/.cache/lanes/claude-375/eval/bench/audit/codegraph_tools.json` | `~/.cache/lanes/claude-375/eval/codegraph/src/README.md:589` | `tools/list` |
| sem | tools/list | dependents_audit | `~/.cache/lanes/claude-375/eval/bench/audit/sem_tools.json` | `~/.cache/lanes/claude-375/eval/sem/src/crates/sem-mcp/src/tools.rs:96` | `tools/list` |
| cbm | tools/list | trace_audit | `~/.cache/lanes/claude-375/eval/bench/audit/cbm_tools.json` | `~/.cache/lanes/claude-375/eval/cbm/src/src/mcp/mcp.c:533` | `tools/list` |
| graphify | tools/list | neighbors2_audit | `~/.cache/lanes/claude-375/eval/bench/audit/graphify_tools.json` | `~/.cache/lanes/claude-375/eval/graphify/src/graphify/serve.py:1912` | `tools/list` |
| codegraph | release | callers_audit | `~/.cache/lanes/claude-375/eval/codegraph/src/CHANGELOG.md:390` | `~/.cache/lanes/claude-375/eval/codegraph/src/src/graph/traversal.ts:279` | `codegraph --version` |
| cbm | release | trace_audit | `https://github.com/DeusData/codebase-memory-mcp/releases/tag/v0.11.0` | `~/.cache/lanes/claude-375/eval/cbm/src/src/mcp/mcp.c:484` | `codebase-memory-mcp --version` |
| sem | release | dependents_audit | `~/.cache/lanes/claude-375/eval/sem/src/CHANGELOG.md:24` | `~/.cache/lanes/claude-375/eval/sem/src/crates/sem-cli/src/commands/query.rs:118` | `sem --version` |
| graphify | parser | query_audit | `~/.cache/lanes/claude-375/eval/bench/score.py:206` | `~/.cache/lanes/claude-375/eval/bench/audit/graphify_parse.py:4` | `score.py query_audit` |
| cbm | parser | trace_audit | `~/.cache/lanes/claude-375/eval/bench/score.py:74` | `~/.cache/lanes/claude-375/eval/bench/audit/cbm_parse.py:4` | `score.py trace_audit` |
| graphify | release | affected_id_audit | `~/.cache/lanes/claude-375/eval/graphify/src/CHANGELOG.md:5` | `~/.cache/lanes/claude-375/eval/graphify/src/graphify/affected.py:138` | `graphify --version` |

| tool | endpoint | kind | target | miss_class | truth_site | evidence_path | observation |
| --- | --- | --- | --- | --- | --- | --- | --- |
| codegraph | callers_audit | fn | codegraph-src/getChildByField | usage | `src/extraction/function-ref.ts:35` | `~/.cache/lanes/claude-375/eval/bench/out/codegraph-src.codegraph.callers_audit.048d587665c5.out` | result cap raised file recall from 0.2273 to 1.0000 |
| codegraph | callers_audit | fn | hafley_scm/io_path | tool limitation | `src/read/1_reach.rs:45` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.codegraph.callers_audit.0c772691301c.out` | indexed caller array is empty |
| codegraph | callers_audit | type | hafley_scm/GoRun | tool limitation | `src/read/scip.rs:293` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.codegraph.callers_audit.c8bd05e48eae.out` | indexed caller result omits the SCIP type reference |
| codegraph | callers_audit | term | hafley_scm/direct | usage | `src/read/lang/6_scm_family.rs:96` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.codegraph.callers_audit.68545932c5ad.out` | caller lookup is documented for callable symbols |
| sem | dependents_audit | fn | tokio/can_auto_advance | tool limitation | `tokio/src/runtime/time/mod.rs:263` | `~/.cache/lanes/claude-375/eval/bench/out/tokio.sem.dependents_audit.597e1125c833.out.err` | lookup index omits the method |
| sem | dependents_audit | type | hafley_scm/GoRun | tool limitation | `src/read/scip.rs:293` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.sem.dependents_audit.24c37d0a884f.out.err` | lookup index omits the enum variant |
| sem | dependents_audit | term | hafley_scm/direct | tool limitation | `src/read/lang/6_scm_family.rs:96` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.sem.dependents_audit.95d32c7ee83c.out.err` | lookup index omits the field |
| graphify | affected_id_audit | fn | codegraph-src/extractFunction | usage | `src/extraction/tree-sitter.ts:1154` | `~/.cache/lanes/claude-375/eval/bench/out/codegraph-src.graphify.affected_id_audit.7258be21c81e.out` | exact node ID raises file recall from zero to one |
| graphify | affected_id_audit | fn | hafley_scm/io_path | tool limitation | `src/read/1_reach.rs:45` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.affected_id_audit.be7581df118f.out` | graph has no incoming reference edge |
| graphify | affected_id_audit | type | hafley_scm/GoRun | tool limitation | `src/read/scip.rs:293` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.affected_id_audit.301333c6e883.out` | graph has no matching enum variant node |
| graphify | neighbors2_audit | term | hafley_scm/direct | tool limitation | `src/read/lang/6_scm_family.rs:96` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.neighbors2_audit.4cc1ee1f2ea5.out` | graph has no matching field node |
| graphify | query | fn | hafley_scm/node_span | scorer/parser | `src/read/lang/commonlisp/_0_source.rs:16` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.query.d4abf8d14fec.out` | original parser counts unrelated BFS nodes |
| graphify | query_audit | type | hafley_scm/GoRun | usage | `src/read/scip.rs:293` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.query_audit.3b0962b67bc4.out` | call-context traversal emits no type reference edge |
| graphify | query_audit | term | hafley_scm/direct | usage | `src/read/lang/6_scm_family.rs:96` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.query_audit.96224e007992.out` | call-context traversal emits no field reference edge |
| cbm | trace_audit | type | vite/ShimOptions | scorer/parser | `packages/vite/rolldown.config.ts:230` | `~/.cache/lanes/claude-375/eval/bench/out/vite.cbm.trace_audit.82ef11724b35.out` | dotted TypeScript filename maps to the truth file |
| cbm | trace_audit | type | codegraph-src/WireArm | usage | `src/ui-server/api/program.ts:59` | `~/.cache/lanes/claude-375/eval/bench/out/codegraph-src.cbm.trace_audit.845932c859ad.out` | qualified name resolves the indexed type |
| cbm | trace_audit | term | hafley-rs/top | usage | `crates/boop-harness/src/pane.rs:238` | `~/.cache/lanes/claude-375/eval/bench/out/hafley-rs.cbm.trace_audit.f482d7283d82.out` | qualified name resolves the indexed field |
| cbm | trace_audit | fn | tokio/can_auto_advance | tool limitation | `tokio/src/runtime/time/mod.rs:263` | `~/.cache/lanes/claude-375/eval/bench/out/tokio.cbm.trace_audit.270004c4758a.out` | search and trace report function not found |
| cbm | trace_audit | type | hafley_scm/GoRun | tool limitation | `src/read/scip.rs:293` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.cbm.trace_audit.39d57eaf6531.out` | search and trace report function not found |
| cbm | trace_audit | term | hafley_scm/DEFINITION | tool limitation | `src/read/2_slow.rs:186` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.cbm.trace_audit.364929bc1b01.out` | search and trace report function not found |
| cbm | trace_audit | term | tokio/node | tool limitation | `tokio/src/sync/batch_semaphore.rs:625` | `~/.cache/lanes/claude-375/eval/bench/out/tokio.cbm.trace_audit.e94fb9b360cd.out` | trace includes an unrelated file |
| codegraph | callers | fn | codegraph-src/getChildByField | usage | `src/extraction/function-ref.ts:35` | `~/.cache/lanes/claude-375/eval/bench/out/codegraph-src.codegraph.callers.41c141e6cd8c.out` | baseline cell sample |
| codegraph | callers | type | hafley_scm/GoRun | usage | `src/read/scip.rs:293` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.codegraph.callers.cb38dac1535f.out` | baseline cell sample |
| codegraph | callers | term | hafley_scm/direct | usage | `src/read/lang/6_scm_family.rs:96` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.codegraph.callers.37adba9c90f6.out` | baseline cell sample |
| codegraph | explore | fn | hafley_scm/io_path | usage | `src/read/1_reach.rs:45` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.codegraph.explore.d02b12bb37e6.out` | baseline cell sample |
| codegraph | explore | type | hafley_scm/GoRun | usage | `src/read/scip.rs:293` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.codegraph.explore.c60b9b4fc563.out` | baseline cell sample |
| codegraph | explore | term | hafley_scm/direct | usage | `src/read/lang/6_scm_family.rs:96` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.codegraph.explore.1a288a2577df.out` | baseline cell sample |
| sem | callers | fn | tokio/can_auto_advance | tool limitation | `tokio/src/runtime/time/mod.rs:263` | `~/.cache/lanes/claude-375/eval/bench/out/tokio.sem.callers.ab07d934d8e0.out` | baseline cell sample |
| sem | callers | type | hafley_scm/GoRun | tool limitation | `src/read/scip.rs:293` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.sem.callers.9db5a58b7f31.out` | baseline cell sample |
| sem | callers | term | hafley_scm/direct | tool limitation | `src/read/lang/6_scm_family.rs:96` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.sem.callers.ceaf3ad6e169.out` | baseline cell sample |
| sem | dependents_audit | fn | tokio/can_auto_advance | tool limitation | `tokio/src/runtime/time/mod.rs:263` | `~/.cache/lanes/claude-375/eval/bench/out/tokio.sem.dependents_audit.597e1125c833.out.err` | baseline cell sample |
| sem | dependents_audit | type | hafley_scm/GoRun | tool limitation | `src/read/scip.rs:293` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.sem.dependents_audit.24c37d0a884f.out.err` | baseline cell sample |
| sem | dependents_audit | term | hafley_scm/direct | tool limitation | `src/read/lang/6_scm_family.rs:96` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.sem.dependents_audit.95d32c7ee83c.out.err` | baseline cell sample |
| graphify | affected_id_audit | fn | codegraph-src/extractFunction | usage | `src/extraction/tree-sitter.ts:1154` | `~/.cache/lanes/claude-375/eval/bench/out/codegraph-src.graphify.affected_id_audit.7258be21c81e.out` | baseline cell sample |
| graphify | affected_id_audit | type | hafley_scm/GoRun | tool limitation | `src/read/scip.rs:293` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.affected_id_audit.301333c6e883.out` | baseline cell sample |
| graphify | affected_id_audit | term | hafley_scm/direct | tool limitation | `src/read/lang/6_scm_family.rs:96` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.affected_id_audit.4716c94a0d45.out` | baseline cell sample |
| graphify | explain | fn | hafley_scm/type_refs | usage | `src/lang/rust/7_type_entity_rows.rs:4` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.explain.8f4330c51945.out` | baseline cell sample |
| graphify | explain | type | hafley_scm/GoRun | usage | `src/read/scip.rs:293` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.explain.b8328da47845.out` | baseline cell sample |
| graphify | explain | term | hafley_scm/direct | usage | `src/read/lang/6_scm_family.rs:96` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.explain.8224a8bec38f.out` | baseline cell sample |
| graphify | query | fn | hafley_scm/node_span | scorer/parser | `src/read/lang/commonlisp/_0_source.rs:16` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.query.d4abf8d14fec.out` | baseline cell sample |
| graphify | query | type | hafley_scm/GoRun | scorer/parser | `src/read/scip.rs:293` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.query.43377716f638.out` | baseline cell sample |
| graphify | query | term | hafley_scm/direct | scorer/parser | `src/read/lang/6_scm_family.rs:96` | `~/.cache/lanes/claude-375/eval/bench/out/hafley_scm.graphify.query.fbbce109676f.out` | baseline cell sample |
| cbm | trace_audit | type | codegraph-src/WireArm | usage | `src/ui-server/api/program.ts:59` | `~/.cache/lanes/claude-375/eval/bench/out/codegraph-src.cbm.trace_audit.845932c859ad.out` | baseline cell sample |
| cbm | trace_audit | term | hafley-rs/top | usage | `crates/boop-harness/src/pane.rs:238` | `~/.cache/lanes/claude-375/eval/bench/out/hafley-rs.cbm.trace_audit.f482d7283d82.out` | baseline cell sample |
| codegraph | index_check | fn | hafley_scm/io_path | tool limitation | `src/read/1_reach.rs:45` | `~/.cache/lanes/claude-375/eval/bench/audit/codegraph_io_path_index_check.json` | definition indexed; caller array empty |
| sem | index_check | fn | tokio/can_auto_advance | tool limitation | `tokio/src/runtime/time/mod.rs:263` | `~/.cache/lanes/claude-375/eval/bench/audit/sem_clock_entities.json` | file entity extraction returns zero |
| cbm | index_check | fn | tokio/can_auto_advance | tool limitation | `tokio/src/runtime/time/mod.rs:263` | `~/.cache/lanes/claude-375/eval/bench/audit/cbm_clock_index_check2.json` | source file indexed only as file node |
