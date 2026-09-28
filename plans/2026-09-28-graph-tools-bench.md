# Benchmark report

Read-only report generated from `bench.db`, `truth.db`, the benchmark scripts, and the four tool reports. Stored metrics below come from SQL queries shown with each result table.

## Method

### Repositories and files

### Distinct SCIP-covered paths by repository

```sql
select repo, count(distinct path) as files from occ group by repo order by repo
```

| Repo | SCIP paths |
| --- | --- |
| codegraph-src | 248 |
| django | 2930 |
| graphify-src | 403 |
| hafley-rs | 456 |
| hafley_scm | 157 |
| requests | 19 |
| tokio | 699 |
| vite | 379 |

File counts are distinct `occ.path` values per repository, so these counts cover paths with SCIP occurrences. `index.sh` attempted all five tools on nine repositories; `gin` has index rows but no `occ` or `target` rows.

### Targets per repository and kind

```sql
select repo, count(*) as targets, sum(kind="fn") as fn, sum(kind="type") as type, sum(kind="term") as term from target group by repo order by repo
```

| Repo | Targets | fn | type | term |
| --- | --- | --- | --- | --- |
| codegraph-src | 14 | 8 | 4 | 2 |
| django | 16 | 8 | 4 | 4 |
| graphify-src | 16 | 8 | 4 | 4 |
| hafley-rs | 16 | 8 | 4 | 4 |
| hafley_scm | 16 | 8 | 4 | 4 |
| requests | 13 | 6 | 3 | 4 |
| tokio | 15 | 8 | 4 | 3 |
| vite | 7 | 4 | 2 | 1 |

### Targets per repo, kind, and reference bucket

```sql
select repo,kind,sum(bucket=0) as bucket_0,sum(bucket=1) as bucket_1,sum(bucket=2) as bucket_2,sum(bucket=3) as bucket_3,count(*) as targets from target group by repo,kind order by repo,kind
```

| Repo | Kind | Bucket 0 | Bucket 1 | Bucket 2 | Bucket 3 | Targets |
| --- | --- | --- | --- | --- | --- | --- |
| codegraph-src | fn | 2 | 2 | 2 | 2 | 8 |
| codegraph-src | term | 0 | 0 | 1 | 1 | 2 |
| codegraph-src | type | 1 | 1 | 1 | 1 | 4 |
| django | fn | 2 | 2 | 2 | 2 | 8 |
| django | term | 1 | 1 | 1 | 1 | 4 |
| django | type | 1 | 1 | 1 | 1 | 4 |
| graphify-src | fn | 2 | 2 | 2 | 2 | 8 |
| graphify-src | term | 1 | 1 | 1 | 1 | 4 |
| graphify-src | type | 1 | 1 | 1 | 1 | 4 |
| hafley-rs | fn | 2 | 2 | 2 | 2 | 8 |
| hafley-rs | term | 1 | 1 | 1 | 1 | 4 |
| hafley-rs | type | 1 | 1 | 1 | 1 | 4 |
| hafley_scm | fn | 2 | 2 | 2 | 2 | 8 |
| hafley_scm | term | 1 | 1 | 1 | 1 | 4 |
| hafley_scm | type | 1 | 1 | 1 | 1 | 4 |
| requests | fn | 2 | 2 | 2 | 0 | 6 |
| requests | term | 1 | 1 | 1 | 1 | 4 |
| requests | type | 1 | 1 | 1 | 0 | 3 |
| tokio | fn | 2 | 2 | 2 | 2 | 8 |
| tokio | term | 1 | 1 | 1 | 0 | 3 |
| tokio | type | 1 | 1 | 1 | 1 | 4 |
| vite | fn | 2 | 2 | 0 | 0 | 4 |
| vite | term | 1 | 0 | 0 | 0 | 1 |
| vite | type | 1 | 1 | 0 | 0 | 2 |

### Targets by reference bucket

```sql
select bucket, count(*) as targets from target group by bucket order by bucket
```

| Bucket | Targets |
| --- | --- |
| 0 | 31 |
| 1 | 30 |
| 2 | 28 |
| 3 | 24 |

Buckets defined in `truth.py`: `0 = 1–2 refs`, `1 = 3–10`, `2 = 11–50`, `3 = 51+`. Per-repository targets are sampled deterministically with `random.Random(repo)` from defined-once names, in-kind candidate order `fn`, `type`, `term`, `fn` for each bucket. Target totals are 113: 58 fn, 29 type, 26 term.

### Truth source

`truth.py` reads `repos/<repo>/index.scip`; it runs `ryii scip --raw --records scip_occurrence --scip-index ... --sqlite ... --root ...` to populate symbol occurrences with path, line, and definition flag. The indices were produced by rust-analyzer, scip-typescript, and scip-python. Local SCIP symbols are excluded. Truth references are distinct `(path, line)` non-definition occurrences. A target requires one definition per name in a repository, at least one reference, kind in `fn/type/term`, name length > 2, and a definition path without `test` or `bench`.

### Index commands

`index.sh` command patterns (with `$B=~/.cache/lanes/claude-375/eval`, `$RB=$B/bench/runs`, `$r` repo):

```sh
$RUN $r ryii index - -- ryii --resolve --sqlite $B/bench/db/$r.ryii.db --root $RB/ryii/$r $RB/ryii/$r
$RUN $r ryii fast - -- ryii fast --sqlite $B/bench/db/$r.ryii-fast.db --root $RB/ryii/$r $RB/ryii/$r
$RUN $r codegraph index - -- $B/codegraph/codegraph init --yes $RB/codegraph/$r
CBM_CACHE_DIR=$B/bench/cbm-cache $RUN $r cbm index - -- $B/cbm/codebase-memory-mcp cli index_repository "{\"repo_path\":\"$RB/cbm/$r\"}"
$RUN $r sem index - -- bash -c "cd $RB/sem/$r && $B/sem/sem find main --json"
$RUN $r graphify index - -- $B/graphify/graphify extract $RB/graphify/$r --code-only --no-cluster --out $RB/graphify/$r/graphify-out
```

### Query argv patterns from `query.py`

Every target is run once per listed tool and endpoint when its endpoint guard passes. `RYII`, `CG`, `SEM`, and `GFY` are executable paths; `$r=$RB/<tool>/<repo>`, `$s=$RB/sem/<repo>`, `$project=$RB/cbm/<repo>` with `/` replaced by `-`, `$graph=$RB/graphify/<repo>/graphify-out/graphify-out/graph.json`, `$name` is target name, `$def_path` its definition path, and `$symbol` its SCIP symbol. Each argv below follows `run.py <repo> <tool> <endpoint> <symbol> -- <argv...>`.

```text
ryii:       [RYII, graph, --callers, $name, --root, $r, $r]
ryii type:  [RYII, graph, --uses, $name, --root, $r, $r]
ryii rename (def suffix .rs/.ts/.tsx/.kt): [RYII, rename, $def_path#$name, ${name}_zz, --root, $r, --json]
codegraph callers: [CG, callers, $name, --json, --path, $RB/codegraph/$repo]
codegraph impact:  [CG, impact, $name, --json, --path, $RB/codegraph/$repo]
codegraph query:   [CG, query, $name, --json, --path, $RB/codegraph/$repo]
codegraph explore: [CG, explore, $name, --path, $RB/codegraph/$repo]
sem callers:    [bash, -c, cd $s && $SEM callers '$name' --file '$def_path' --json]
sem impact:     [bash, -c, cd $s && $SEM impact '$name' --file '$def_path' --json]
sem find:       [bash, -c, cd $s && $SEM find '$name' --json]
sem dependents: [bash, -c, cd $s && $SEM impact '$name' --file '$def_path' --dependents --json]
cbm trace:      [CBM, cli, --json, trace_path, JSON({project:$project,function_name:$name,direction:inbound,depth:1,edge_types:[CALLS,CALL_REFERENCE,USAGE,USES_TYPE,IMPORTS],include_tests:true,format:json,limit:1000})]
cbm search:     [CBM, cli, --json, search_graph, JSON({project:$project,name_pattern:^$name$,format:json,limit:100})]
graphify query:    [GFY, query, 'what calls or uses $name', --graph, $graph]
graphify affected: [GFY, affected, $name, --depth, 1, --graph, $graph]
graphify explain:  [GFY, explain, $name, --graph, $graph]
```

### Caveats

- CBM cold `trace_path` argv includes the corrected explicit edge filter `CALLS`, `CALL_REFERENCE`, `USAGE`, `USES_TYPE`, and `IMPORTS`. `warm.py` MCP `trace_path` arguments omit `edge_types`; cold and warm CBM trace calls therefore differ in that argument.
- Graphify CLI queries use the repeated path `$RB/graphify/<repo>/graphify-out/graphify-out/graph.json`; the warm server uses that same path.
- `warm.py` runs RYii through thin client `ryi` and stores call timing per target. RYii has no MCP server; the daemon re-extracts on each call, so those rows include per-call extraction.
- `run.py` samples process tree RSS every 50 ms; peak RSS can miss short spikes. Index rows are the recorded `endpoint=index` rows; two RYii rows have nonzero `rc=-6` (django and graphify-src).
- `score.py` averages target-level file recall/precision. File truth is the set of files with SCIP reference occurrences; output file paths are intersected with the repository’s distinct SCIP occurrence paths. Undefined precision is NULL when found set is empty. Site-level scores are available only for endpoints whose parser produces sites; sem returns entity spans scored as `ent_hit`.

## File-level precision and recall

### Endpoint x kind (target means)

```sql
select tool, endpoint, kind, count(*) as targets, round(avg(file_r),4) as mean_file_recall, round(avg(file_p),4) as mean_file_precision from score group by tool,endpoint,kind order by tool,endpoint,kind
```

| Tool | Endpoint | Kind | Targets | Mean recall | Mean precision |
| --- | --- | --- | --- | --- | --- |
| cbm | trace | fn | 58 | 0.8035 | 0.9402 |
| cbm | trace | term | 26 | 0.5154 | 0.9935 |
| cbm | trace | type | 29 | 0.6029 | 0.8333 |
| codegraph | callers | fn | 58 | 0.7859 | 0.9566 |
| codegraph | callers | term | 26 | 0.3075 | 0.8095 |
| codegraph | callers | type | 29 | 0.6948 | 0.96 |
| codegraph | explore | fn | 58 | 0.7635 | 0.9317 |
| codegraph | explore | term | 26 | 0.1866 | 0.7619 |
| codegraph | explore | type | 29 | 0.5767 | 0.9667 |
| graphify | affected | fn | 58 | 0.6325 | 0.9756 |
| graphify | affected | term | 26 | 0.0 | 0.0 |
| graphify | affected | type | 29 | 0.5695 | 0.9833 |
| graphify | explain | fn | 58 | 0.5914 | 0.8007 |
| graphify | explain | term | 26 | 0.0096 | 0.5 |
| graphify | explain | type | 29 | 0.5357 | 0.9825 |
| graphify | query | fn | 58 | 0.5186 | 0.3145 |
| graphify | query | term | 26 | 0.2353 | 0.1791 |
| graphify | query | type | 29 | 0.4848 | 0.6678 |
| ryii | callers | fn | 58 | 0.7684 | 0.9773 |
| ryii | callers | term | 26 | 0.0 | — |
| ryii | callers | type | 29 | 0.1006 | 1.0 |
| ryii | rename | fn | 36 | 0.4978 | 0.685 |
| ryii | rename | term | 14 | 0.6139 | 0.9 |
| ryii | rename | type | 18 | 0.7685 | 0.9651 |
| ryii | uses | type | 29 | 0.5436 | 0.9684 |
| sem | callers | fn | 58 | 0.7416 | 0.9769 |
| sem | callers | term | 26 | 0.0769 | 1.0 |
| sem | callers | type | 29 | 0.5697 | 1.0 |
| sem | dependents | fn | 58 | 0.7416 | 0.9769 |
| sem | dependents | term | 26 | 0.0769 | 1.0 |
| sem | dependents | type | 29 | 0.5697 | 1.0 |

## Per repository

### Repo x tool x endpoint (target means)

```sql
select repo,tool,endpoint,count(*) as targets,round(avg(file_r),4) as mean_file_recall,round(avg(file_p),4) as mean_file_precision from score group by repo,tool,endpoint order by repo,tool,endpoint
```

| Repo | Tool | Endpoint | Targets | Mean recall | Mean precision |
| --- | --- | --- | --- | --- | --- |
| codegraph-src | cbm | trace | 14 | 0.7098 | 0.9167 |
| codegraph-src | codegraph | callers | 14 | 0.7171 | 0.9231 |
| codegraph-src | codegraph | explore | 14 | 0.6737 | 1.0 |
| codegraph-src | graphify | affected | 14 | 0.4643 | 1.0 |
| codegraph-src | graphify | explain | 14 | 0.4476 | 0.8 |
| codegraph-src | graphify | query | 14 | 0.5617 | 0.5165 |
| codegraph-src | ryii | callers | 14 | 0.5 | 1.0 |
| codegraph-src | ryii | rename | 14 | 0.4286 | 0.9829 |
| codegraph-src | ryii | uses | 4 | 0.9844 | 1.0 |
| codegraph-src | sem | callers | 14 | 0.7098 | 1.0 |
| codegraph-src | sem | dependents | 14 | 0.7098 | 1.0 |
| django | cbm | trace | 16 | 0.7674 | 0.8596 |
| django | codegraph | callers | 16 | 0.6372 | 0.9056 |
| django | codegraph | explore | 16 | 0.6288 | 0.8778 |
| django | graphify | affected | 16 | 0.4844 | 1.0 |
| django | graphify | explain | 16 | 0.5781 | 0.775 |
| django | graphify | query | 16 | 0.3069 | 0.3213 |
| django | ryii | callers | 16 | 0.425 | 0.8381 |
| django | ryii | uses | 4 | 0.5 | 1.0 |
| django | sem | callers | 16 | 0.6635 | 0.9056 |
| django | sem | dependents | 16 | 0.6635 | 0.9056 |
| graphify-src | cbm | trace | 16 | 0.6681 | 0.9699 |
| graphify-src | codegraph | callers | 16 | 0.7755 | 0.9556 |
| graphify-src | codegraph | explore | 16 | 0.6871 | 0.8462 |
| graphify-src | graphify | affected | 16 | 0.686 | 1.0 |
| graphify-src | graphify | explain | 16 | 0.5972 | 1.0 |
| graphify-src | graphify | query | 16 | 0.6931 | 0.2879 |
| graphify-src | ryii | callers | 16 | 0.4261 | 1.0 |
| graphify-src | ryii | uses | 4 | 0.3333 | 1.0 |
| graphify-src | sem | callers | 16 | 0.6755 | 1.0 |
| graphify-src | sem | dependents | 16 | 0.6755 | 1.0 |
| hafley-rs | cbm | trace | 16 | 0.7147 | 1.0 |
| hafley-rs | codegraph | callers | 16 | 0.608 | 1.0 |
| hafley-rs | codegraph | explore | 16 | 0.4998 | 0.8833 |
| hafley-rs | graphify | affected | 16 | 0.3397 | 1.0 |
| hafley-rs | graphify | explain | 16 | 0.2407 | 0.75 |
| hafley-rs | graphify | query | 16 | 0.1806 | 0.2949 |
| hafley-rs | ryii | callers | 16 | 0.4397 | 1.0 |
| hafley-rs | ryii | rename | 16 | 0.6258 | 0.7278 |
| hafley-rs | ryii | uses | 4 | 0.375 | 1.0 |
| hafley-rs | sem | callers | 16 | 0.4293 | 1.0 |
| hafley-rs | sem | dependents | 16 | 0.4293 | 1.0 |
| hafley_scm | cbm | trace | 16 | 0.7192 | 0.9615 |
| hafley_scm | codegraph | callers | 16 | 0.5924 | 0.9231 |
| hafley_scm | codegraph | explore | 16 | 0.4505 | 0.95 |
| hafley_scm | graphify | affected | 16 | 0.3999 | 0.9 |
| hafley_scm | graphify | explain | 16 | 0.3021 | 0.8571 |
| hafley_scm | graphify | query | 16 | 0.3984 | 0.3412 |
| hafley_scm | ryii | callers | 16 | 0.5065 | 1.0 |
| hafley_scm | ryii | rename | 16 | 0.6914 | 0.7926 |
| hafley_scm | ryii | uses | 4 | 0.3606 | 1.0 |
| hafley_scm | sem | callers | 16 | 0.4276 | 1.0 |
| hafley_scm | sem | dependents | 16 | 0.4276 | 1.0 |
| requests | cbm | trace | 13 | 0.7564 | 1.0 |
| requests | codegraph | callers | 13 | 0.6538 | 1.0 |
| requests | codegraph | explore | 13 | 0.6128 | 1.0 |
| requests | graphify | affected | 13 | 0.6795 | 1.0 |
| requests | graphify | explain | 13 | 0.6987 | 0.9 |
| requests | graphify | query | 13 | 0.5256 | 0.2781 |
| requests | ryii | callers | 13 | 0.3692 | 1.0 |
| requests | ryii | uses | 3 | 0.5556 | 1.0 |
| requests | sem | callers | 13 | 0.6128 | 1.0 |
| requests | sem | dependents | 13 | 0.6128 | 1.0 |
| tokio | cbm | trace | 15 | 0.6539 | 0.7655 |
| tokio | codegraph | callers | 15 | 0.4919 | 0.8229 |
| tokio | codegraph | explore | 15 | 0.4133 | 0.9125 |
| tokio | graphify | affected | 15 | 0.354 | 0.8333 |
| tokio | graphify | explain | 15 | 0.3496 | 0.7778 |
| tokio | graphify | query | 15 | 0.2863 | 0.3899 |
| tokio | ryii | callers | 15 | 0.195 | 1.0 |
| tokio | ryii | rename | 15 | 0.4182 | 0.7197 |
| tokio | ryii | uses | 4 | 0.4708 | 0.8 |
| tokio | sem | callers | 15 | 0.1662 | 1.0 |
| tokio | sem | dependents | 15 | 0.1662 | 1.0 |
| vite | cbm | trace | 7 | 0.2857 | 1.0 |
| vite | codegraph | callers | 7 | 0.8571 | 1.0 |
| vite | codegraph | explore | 7 | 0.8571 | 1.0 |
| vite | graphify | affected | 7 | 0.2857 | 1.0 |
| vite | graphify | explain | 7 | 0.2857 | 1.0 |
| vite | graphify | query | 7 | 0.8571 | 0.8571 |
| vite | ryii | callers | 7 | 0.5714 | 1.0 |
| vite | ryii | rename | 7 | 1.0 | 1.0 |
| vite | ryii | uses | 2 | 1.0 | 1.0 |
| vite | sem | callers | 7 | 0.8571 | 1.0 |
| vite | sem | dependents | 7 | 0.8571 | 1.0 |

## Site-level scores

### Endpoints with site or entity-span scoring

```sql
select tool,endpoint,kind,sum(case when site_r is not null then 1 else 0 end) as site_scored_targets,round(avg(site_r),4) as mean_site_recall,round(avg(site_p),4) as mean_site_precision,round(avg(ent_hit),4) as mean_entity_span_hit from score where site_r is not null or ent_hit is not null group by tool,endpoint,kind order by tool,endpoint,kind
```

| Tool | Endpoint | Kind | Site targets | Mean site recall | Mean site precision | Mean entity-span hit |
| --- | --- | --- | --- | --- | --- | --- |
| graphify | affected | fn | 58 | 0.4166 | 0.9459 | — |
| graphify | affected | term | 26 | 0.0 | 0.0 | — |
| graphify | affected | type | 29 | 0.2891 | 0.7911 | — |
| graphify | explain | fn | 58 | 0.3761 | 0.6657 | — |
| graphify | explain | term | 26 | 0.0052 | 0.1875 | — |
| graphify | explain | type | 29 | 0.2285 | 0.6959 | — |
| ryii | callers | fn | 58 | 0.7191 | 0.9906 | — |
| ryii | callers | term | 26 | 0.0 | — | — |
| ryii | callers | type | 29 | 0.0473 | 1.0 | — |
| ryii | rename | fn | 36 | 0.476 | 0.5278 | — |
| ryii | rename | term | 14 | 0.4921 | 0.6066 | — |
| ryii | rename | type | 18 | 0.746 | 0.7452 | — |
| ryii | uses | type | 29 | 0.2518 | 0.6286 | — |
| sem | callers | fn | 0 | — | — | 0.8267 |
| sem | callers | term | 0 | — | — | 1.0 |
| sem | callers | type | 0 | — | — | 0.8259 |
| sem | dependents | fn | 0 | — | — | 0.8267 |
| sem | dependents | term | 0 | — | — | 1.0 |
| sem | dependents | type | 0 | — | — | 0.8259 |

## Index time and peak RSS

### Recorded `endpoint=index` rows

```sql
select repo,tool,round(wall_s,3) as seconds,rc,peak_rss_mb as peak_rss_mib,out_bytes as stdout_bytes from run where endpoint='index' order by repo,tool
```

| Repo | Tool | Seconds | rc | Peak RSS MiB | stdout bytes |
| --- | --- | --- | --- | --- | --- |
| codegraph-src | cbm | 36.298 | 0 | 2453.4 | 2108 |
| codegraph-src | codegraph | 9.719 | 0 | 2184.3 | 305 |
| codegraph-src | graphify | 24.661 | 0 | 2852.5 | 1328 |
| codegraph-src | ryii | 4.11 | 0 | 441.5 | 703 |
| codegraph-src | sem | 4.852 | 0 | 2010.8 | 1409 |
| django | cbm | 21.379 | 0 | 983.7 | 3076 |
| django | codegraph | 9.012 | 0 | 2021.3 | 301 |
| django | graphify | 39.628 | 0 | 1383.0 | 2839 |
| django | ryii | 12.142 | -6 | 946.3 | 0 |
| django | sem | 1.611 | 0 | 908.5 | 842 |
| gin | cbm | 14.141 | 0 | 211.0 | 1026 |
| gin | codegraph | 1.475 | 0 | 348.2 | 294 |
| gin | graphify | 2.009 | 0 | 565.4 | 900 |
| gin | ryii | 0.756 | 0 | 92.7 | 652 |
| gin | sem | 0.357 | 0 | 92.3 | 3 |
| graphify-src | cbm | 14.614 | 0 | 520.5 | 2160 |
| graphify-src | codegraph | 4.567 | 0 | 1273.2 | 304 |
| graphify-src | graphify | 9.361 | 0 | 1028.3 | 1067 |
| graphify-src | ryii | 4.056 | -6 | 364.9 | 0 |
| graphify-src | sem | 0.868 | 0 | 392.7 | 881 |
| hafley-rs | cbm | 17.321 | 0 | 610.8 | 2892 |
| hafley-rs | codegraph | 6.206 | 0 | 1562.9 | 304 |
| hafley-rs | graphify | 9.262 | 0 | 974.5 | 1903 |
| hafley-rs | ryii | 6.971 | 0 | 500.6 | 683 |
| hafley-rs | sem | 0.882 | 0 | 654.4 | 8247 |
| hafley_scm | cbm | 13.852 | 0 | 214.9 | 603 |
| hafley_scm | codegraph | 2.122 | 0 | 507.6 | 302 |
| hafley_scm | graphify | 1.897 | 0 | 702.8 | 782 |
| hafley_scm | ryii | 2.13 | 0 | 118.0 | 688 |
| hafley_scm | sem | 0.408 | 0 | 183.1 | 158 |
| requests | cbm | 11.245 | 0 | 161.5 | 1474 |
| requests | codegraph | 1.039 | 0 | 359.2 | 298 |
| requests | graphify | 1.341 | 0 | 597.4 | 878 |
| requests | ryii | 3.628 | 0 | 75.9 | 677 |
| requests | sem | 1.214 | 0 | 161.4 | 142 |
| tokio | cbm | 12.257 | 0 | 374.0 | 600 |
| tokio | codegraph | 3.08 | 0 | 973.2 | 298 |
| tokio | graphify | 4.017 | 0 | 781.6 | 1243 |
| tokio | ryii | 2.789 | 0 | 238.6 | 663 |
| tokio | sem | 0.467 | 0 | 305.5 | 6078 |
| vite | cbm | 21.925 | 0 | 374.1 | 3330 |
| vite | codegraph | 2.106 | 0 | 806.8 | 707 |
| vite | graphify | 8.124 | 0 | 829.6 | 2194 |
| vite | ryii | 1.873 | 0 | 229.1 | 658 |
| vite | sem | 0.513 | 0 | 311.3 | 19136 |

## Warm calls

### Per-call latency grouped by tool and MCP/client call

```sql
select tool,call,count(*) as calls,round(avg(ms),2) as mean_ms,round(min(ms),2) as min_ms,round(max(ms),2) as max_ms,sum(is_error) as errors from warm group by tool,call order by tool,call
```

| Tool | Call | Calls | Mean ms | Min ms | Max ms | Errors |
| --- | --- | --- | --- | --- | --- | --- |
| cbm | search_graph | 113 | 17.11 | 11.0 | 26.22 | 0 |
| cbm | trace_path | 113 | 17.78 | 11.03 | 151.73 | 16 |
| codegraph | codegraph_callers | 113 | 20.39 | 0.43 | 453.77 | 0 |
| codegraph | codegraph_explore | 113 | 89.13 | 8.74 | 721.2 | 0 |
| codegraph | codegraph_impact | 113 | 1.82 | 0.29 | 27.0 | 0 |
| codegraph | codegraph_search | 113 | 1.07 | 0.21 | 26.47 | 0 |
| graphify | get_neighbors | 113 | 3.54 | 0.51 | 235.4 | 0 |
| graphify | get_node | 113 | 2.98 | 0.51 | 236.21 | 0 |
| graphify | query_graph | 113 | 177.52 | 2.7 | 864.65 | 0 |
| ryi | graph_callers | 113 | 2162.46 | 538.77 | 4691.45 | 32 |
| sem | sem_callers | 113 | 480.13 | 0.08 | 33010.64 | 33 |
| sem | sem_find | 113 | 0.21 | 0.08 | 1.01 | 27 |
| sem | sem_impact | 113 | 7.17 | 0.1 | 26.96 | 27 |

### Warm server initialization and retained process tree

```sql
select tool,count(*) as repos,round(avg(startup_ms),2) as mean_startup_ms,round(avg(procs),1) as mean_procs,round(avg(rss_mb),1) as mean_rss_mib from warm_server group by tool order by tool
```

| Tool | Repos | Mean startup ms | Mean procs | Mean RSS MiB |
| --- | --- | --- | --- | --- |
| cbm | 8 | 7672.34 | 2.0 | 18.9 |
| codegraph | 8 | 165.61 | 4.0 | 403.7 |
| graphify | 8 | 1110.02 | 1.0 | 229.8 |
| sem | 8 | 50.85 | 2.0 | 298.2 |

`warm` call latency excludes initialization, which is stored in `warm_server`. `ryi` rows are per process invocation, not MCP calls.

## `sweep_pair` results

### Pair x tool endpoint, across recorded repositories

```sql
select s.pair,s.tool,s.endpoint,count(*) as repos,sum(case when r.rc=0 then 1 else 0 end) as rc0,sum(case when r.rc<>0 then 1 else 0 end) as nonzero_rc,round(avg(r.wall_s),3) as mean_seconds from sweep_pair s join run r on r.repo=s.repo and r.tool=s.tool and r.endpoint=s.endpoint and r.arg='sweep:'||s.pair group by s.pair,s.tool,s.endpoint order by s.pair,s.tool,s.endpoint
```

| Pair | Tool | Endpoint | Repos | rc=0 | rc!=0 | Mean seconds |
| --- | --- | --- | --- | --- | --- | --- |
| ad_hoc_query | cbm | query_graph | 8 | 8 | 0 | 7.806 |
| ad_hoc_query | ryii | sql | 8 | 6 | 2 | 0.06 |
| architecture | cbm | get_architecture | 8 | 8 | 0 | 7.633 |
| architecture | graphify | god_nodes | 8 | 8 | 0 | 0.355 |
| architecture | ryii | sql_top_degree | 8 | 6 | 2 | 0.06 |
| architecture | ryii | stratify | 8 | 0 | 8 | 2.478 |
| file_dependents | codegraph | affected | 8 | 8 | 0 | 0.385 |
| file_dependents | ryii | sql_reverse_walk | 8 | 6 | 2 | 0.059 |
| file_list | codegraph | files | 8 | 8 | 0 | 0.252 |
| file_list | ryii | sql_files | 8 | 6 | 2 | 0.06 |
| file_outline | cbm | get_file_outline | 8 | 8 | 0 | 7.701 |
| file_outline | ryii | fast_file | 8 | 8 | 0 | 0.138 |
| file_outline | sem | entities | 8 | 8 | 0 | 0.081 |
| history | ryii | none | 8 | 8 | 0 | 0.06 |
| history | sem | blame | 8 | 8 | 0 | 0.087 |
| history | sem | log | 8 | 8 | 0 | 0.124 |
| index_status | cbm | check_index_coverage | 8 | 0 | 8 | 8.427 |
| index_status | cbm | index_status | 8 | 8 | 0 | 7.752 |
| index_status | codegraph | status | 8 | 0 | 8 | 0.219 |
| index_status | ryii | sql_coverage | 8 | 6 | 2 | 0.059 |
| path_between | cbm | trace_outbound | 8 | 7 | 1 | 7.639 |
| path_between | graphify | path | 8 | 5 | 3 | 0.858 |
| path_between | ryii | call_path | 8 | 8 | 0 | 2.297 |
| schema | cbm | get_graph_schema | 8 | 8 | 0 | 7.735 |
| schema | ryii | schema | 8 | 8 | 0 | 0.06 |
| source_context | cbm | get_code_snippet | 8 | 8 | 0 | 7.74 |
| source_context | codegraph | context | 8 | 8 | 0 | 0.253 |
| source_context | codegraph | explore | 8 | 8 | 0 | 0.274 |
| source_context | sem | context | 8 | 8 | 0 | 0.08 |
| text_search | cbm | search_code | 8 | 8 | 0 | 8.006 |
| text_search | ryii | none_rg | 8 | 8 | 0 | 0.081 |
| text_search | sem | grep | 8 | 8 | 0 | 0.111 |

The `sweep_pair` pairing itself is saved by `sweep.py`; for one function target per repo it records the following compared endpoint names and counts:

### Pair membership

```sql
select pair, count(*) as tool_endpoints from sweep_pair group by pair order by pair
```

| Pair | Tool endpoints |
| --- | --- |
| ad_hoc_query | 16 |
| architecture | 32 |
| file_dependents | 16 |
| file_list | 16 |
| file_outline | 24 |
| history | 24 |
| index_status | 32 |
| path_between | 24 |
| schema | 16 |
| source_context | 32 |
| text_search | 24 |

## RYii caller recall gaps

Gap definition: for each target with a `ryii/callers` score, compare its file recall with the maximum file recall from non-RYii scored use endpoints for that same repo and symbol (`callers`, `trace`, `dependents`, `affected`, `explain`, `query`, `explore`). A target is a gap when RYii recall is lower. The 63 targets split by occurrence kind. Causes below are grouped from the gap kind and inspected `out_path` samples.

### Gap counts by kind

```sql
with ry as (select s.repo,s.symbol,s.kind,s.file_r from score s where s.tool='ryii' and s.endpoint='callers'), alt as (select repo,symbol,kind,max(file_r) as best_other_recall from score where tool!='ryii' and endpoint in ('callers','trace','dependents','affected','explain','query','explore') group by repo,symbol,kind) select ry.kind,count(*) as compared_targets,sum(case when ry.file_r<alt.best_other_recall then 1 else 0 end) as gap_targets from ry join alt using(repo,symbol,kind) group by ry.kind order by ry.kind
```

| Kind / cause group | Compared targets | Gap targets |
| --- | --- | --- |
| fn | 58 | 16 |
| term | 26 | 21 |
| type | 29 | 26 |

Cause group counts (these correspond to the gap counts above):

| Cause group | Targets | Cause evidence / sample `out_path` |
| --- | --- | --- |
| Function call graph coverage gaps | 16 | Function targets whose caller results have lower file recall than another tool. Example `tokio/unbounded_channel`: RYii returns three graph edges for 25 truth files (recall 0.12), while CBM trace reaches 0.92. `bench/out/tokio.ryii.callers.4cae4fe93294.out` |
| Terms queried as callers | 21 | Term targets include fields, constants, and module values, while `graph --callers` returns call/name-resolution edges. Example `_url_module_exception` has empty stdout: `bench/out/django.ryii.callers.c523b4d14398.out`. |
| Types queried as callers | 26 | Type references/imports are tested through `graph --callers`; example `FamilyMask` stdout contains `name_resolve` and `import_resolve` edges. The dedicated `graph --uses` endpoint is measured separately in the accuracy tables: `bench/out/hafley-rs.ryii.callers.d08f1f7db266.out`. |
These are caller-endpoint gaps, not a target count that combines RYii `callers`, `uses`, and `rename`. `ryii/uses` has 29 type target scores; SQL result: mean file recall 0.5436, mean file precision 0.9684.

