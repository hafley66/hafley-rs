# Comparative bench

Every comparison of ryi against oracles (tsserver, rust-analyzer) and rivals (Serena, tokensave).

- `../schema/bench/0_bench.tsp`: the domain (repo, target, adapter, lab, row). `just gen` in `crates/sprefa-extract` writes
  `../schema/generated/8_bench.sql` (tables) and `8_bench.json` (enum values and columns).
- `0_bench_db.py`: opens `bench.db` from that DDL and rejects values outside the enums.
- `1_import_lab_results.py BENCH_DB LAB RESULTS_DB ...`: loads a lab's `results.db` (`corpus_score`) into `row`.
- `corpus/`: the topology corpus (repos, targets, CycloneDX SBOMs).
- `labs/<name>/`: each lab's runner and adapters, as the lab left them.
- `adapters/`: Serena MCP client, tsserver rename capture.
- `scripts/`: earlier single-purpose benches.
- `recon/`: the read-only recon runner and its rules.
- `dogfood.tsv`: every structural edit to this tree and the tool that made it.

```sh
python3 bench/1_import_lab_results.py bench.db topology-corpus-2 path/to/results.db
sqlite3 -header -column bench.db "select repo, operation, tool, count(*) targets, sum(status='pass') pass from row group by 1,2,3"
```
