# ryiii bench: TypeSpec contract, generated storage, Rust runner

Step 1 (moves) is commit `8a364b5d`. This is step 2 of `TASKS/bench-consolidate.BRIEF.md`.

## Existing generators this plan extends (no new generator)

| generator | input | output | change needed |
| --- | --- | --- | --- |
| `schema/cli/0_gen.py` + alloy-rs emitter | `schema/cli/ops.tsp` (namespace `Ryi`, `@daemon`) | clap CLI, daemon routes, `ryi-proto` models | compile a second service file, `ryiii.tsp`, into `src/bin/ryiii/gen/` (no daemon, no client) |
| `schema/2_gen.mjs` + `1a_fact_emit.mjs` (`@hafley/typespec-rusqlite`, SQL core) | `schema/1_facts.tsp`, namespace `ExtractSql` | `generated/4_facts.sql`, `7_writers_auto.rs` | `emitFacts` takes the namespace and output names as arguments; a second call emits `BenchSql` into `generated/8_bench.sql` and `9_bench_writers_auto.rs` |

Scalar limits of `emitFacts`: string, boolean, uint32, uint64, int32, int64, enums, Json. There is no float, so durations are `milliseconds: uint32`.

## Type signatures (`schema/bench/0_bench.tsp`)

```tsp
namespace BenchSql;

enum LabKind { recon, bench }
enum Operation { rename, cleave }
enum Status { pass, correct_refusal, tool_failed, typecheck_failed, unexpected_files, timeout, unavailable }
enum Expect { normal, collision_expected, cross_package_refusal }
enum Topology { single_package, monorepo_many_packages, rust_workspace }
enum Runtime { rust, node, mcp }

model repo {            // one pinned checkout
  record: "repo";
  name: string;         // "tokio"
  purl: string;         // "pkg:cargo/tokio@workspace"
  url: string;
  commit: string;
  topology: Topology;
  baseline_command: string;   // shell line, run in the checkout
  baseline_errors: uint32;    // must be 0 to be scored
  baseline_milliseconds: uint32;
}

model target {
  record: "target";
  repo: string;
  operation: Operation;
  file: string;
  name: string;
  at_byte: uint32;
  dest: string | null;
  dest_new: boolean;
  cross_package: boolean;
  expect: Expect;
  allowed_files: Json;        // string[]; from the oracle's edit set
}

model adapter { record: "adapter"; name: string; runtime: Runtime; }   // ryi_fast, ryi_slow, rust_analyzer, ts_oracle, serena, tokensave

model lab {
  record: "lab";
  name: string;
  kind: LabKind;
  brief: string;              // path
  model: string;              // "gpt-6-luna high" for recon, "" for bench
}

model row {                   // one scored attempt
  record: "row";
  lab: string;
  repo: string;
  operation: Operation;
  tool: string;
  target: string;             // "file#name@at_byte"
  bin_sha: string;            // sha256 of the tool binary, "" for external tools
  status: Status;
  cause: string | null;       // first compiler error or tool refusal, verbatim
  files_touched: uint32;
  allowed_files: uint32;
  milliseconds: uint32;
  timeout_milliseconds: uint32;
  after_errors: uint32;
}
```

Field names are snake_case in every layer (TypeSpec, SQL, Rust, CLI JSON), per the cross-system casing rule.

## CLI (`schema/cli/ryiii.tsp`, `@service namespace RyiDev`, no `@daemon`)

```tsp
op labNew(name: string, kind: LabKind, brief: string): jsonValue;                  // ryiii lab new
op corpusAdd(url: string, commit: string, baseline: string): jsonValue;            // ryiii corpus add
op targetAdd(repo: string, operation: Operation, file: string, name: string, at: uint32, dest?: string): jsonValue;
op targetPick(repo: string, operation: Operation, count: uint32): JsonlStream<jsonValue>;
op run(lab: string, tool?: string[], repo?: string[]): JsonlStream<jsonValue>;     // resumable
op score(lab: string): JsonlStream<jsonValue>;
```

The binary is `ryiii` (`src/bin/ryiii.rs`, `[[bin]]` behind feature `dev`).

## Instance lifetimes

- `repo`, `target`, `adapter`, `lab` rows: created by `corpus add`, `target add/pick` and `lab new`; they live for the life of `bench.db`. A repo's commit never changes: a new commit is a new `repo` row.
- A checkout: `bench/repos/<name>/` (gitignored), created by `corpus add`, reset with `git reset --hard && git clean -fd` after every attempt.
- `row`: written once per (lab, repo, operation, tool, target, bin_sha). A rebuilt ryi has a new `bin_sha`, so its rows are new and old rows stay for comparison.
- A slow-tier process: the runner calls ryi in-process for `ryi_fast` / `ryi_slow`, so one rust-analyzer session stays warm across a repo's targets; cold numbers come from a separate `--cold` run that spawns `ryii` per target.

## Storage and sequence

`bench/bench.db` (gitignored), DDL from `generated/8_bench.sql`, writers from `9_bench_writers_auto.rs`.

Unique keys:
- `repo(name, commit)`
- `target(repo, operation, file, name, at_byte)`
- `row(lab, repo, operation, tool, target, bin_sha)`

`ryiii run`, per target:
1. Read the `row` key; if present, skip (resume).
2. Snapshot `git status` of the checkout.
3. Run the adapter under the target's timeout (3 × `baseline_milliseconds`).
4. Diff `git status` → `files_touched`.
5. Run `baseline_command` under the remaining time → `after_errors`, `cause`.
6. Status, in order:
   - `timeout`;
   - `correct_refusal`, when `expect` says so and the tool refused with that reason;
   - `tool_failed`;
   - `typecheck_failed`;
   - `unexpected_files`;
   - `pass`.
7. Insert `row` in its own transaction.
8. Reset the checkout.

## Harness fixes built in (from the corpus labs)

- Per-target timeout from the measured baseline, never a fixed 30 s.
- A cross-package "must depend on" stop scores `correct_refusal` when `expect = cross_package_refusal`.
- `target pick` takes top-level items only (ryi facts: `Node` with an empty chain).
- `allowed_files` comes from the oracle's edit set, with paths resolved through symlinks.
- A repo whose baseline has errors is refused at `corpus add`, with the errors.

## Port order (one adapter at a time, each proven equal to the Python row)

1. The TypeSpec file, compiled; the generator changes; `just gen-check` green.
2. `ryiii lab new`, `corpus add` (the topology corpus rows imported from `bench/corpus/corpus.tsv`), `target add` (imported from `targets.tsv`).
3. `ryiii run` with `ryi_fast`: reproduce corpus-2 fast rows on codegraph-src and anyhow.
4. `ryi_slow`, then `rust_analyzer` (ra_ap_ide rename, the same code as `rename --slow`), then `ts_oracle` (Node, the moved `.cjs`).
5. `serena` and `tokensave` over `rmcp`.
6. `ryiii score` writes the table and the grid; `target pick`.
7. Delete each Python file once its Rust replacement matches.
