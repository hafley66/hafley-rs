# Brief: watch the watchman

One `just` command prints one table that prices every tracing library and every
flush strategy in `hafley-observe`, measured with the layer ON and the same
binary with it OFF.

The lab is **integrated**, not isolated. It lives inside the
`hafley-observe` crate as real features, real layers, and one bench binary that
consumes the crate's public surface. No `labs/` directory, no standalone copy
of the crate, no vendored fork. A finding that cannot be expressed as a feature
of this crate is not a finding.

## Repo and worktree

Repo `~/projects/hafley-rs`, crate `crates/hafley-observe`. Base
`origin/main` at `3660640`. Other agents work in this repo concurrently: the
root `Cargo.lock` is a known conflict site, and some workspace legs fail for
environmental reasons before you touch anything. Read the `hafley-rs-repo`
skill before the first build and record the pre-existing red legs as your
baseline, so you never report someone else's failure as yours.

## Owned files

- `crates/hafley-observe/src/**` (new layers and sinks)
- `crates/hafley-observe/Cargo.toml` (features and deps)
- `crates/hafley-observe/bench/**` (new: the harness binary)
- `crates/hafley-observe/PLANS/2026-09-21-watch-the-watchman.md` (receipts)
- `crates/hafley-observe/PLANS/2026-09-21-watch-the-watchman.visual.human.unga.md`
- `justfile` (one recipe added, nothing else touched)
- root `Cargo.toml` workspace members, only if the bench needs a member row

Forbidden: every other crate in the workspace, `crates/boop/**`,
`crates/sprefa-extract/**`, `crates/redux/**`, `~/projects/sqlite_ivm/**`,
`~/projects/sprefa/**`. Do not resolve another agent's staged work. If the root
`Cargo.lock` conflicts, take the merge path the `hafley-rs-repo` skill names.

## What exists today (do not re-derive)

`crates/hafley-observe/src/`: `_0_types.rs`, `_1_format.rs`, `_1_init.rs`,
`_2_otlp.rs`, `_3_chrome.rs`, `_4_counts.rs`, `sqlite` (feature-gated,
`5_sqlite.rs`). Features: `default = ["otlp", "sqlite"]`, `otlp` pulls
`tracing-opentelemetry`, `opentelemetry`, `opentelemetry_sdk`,
`opentelemetry-otlp`, all at the 0.32/0.33 line, all with
`default-features = false` and **`features = ["trace"]` only**. There is no
metrics pipeline, no system-metric sampler, and no allocation tracking.

A working sampler already exists elsewhere and must be lifted, not rewritten:
`~/projects/sqlite_ivm/bench/src/arms/mod.rs:117` (`getrusage`, RUSAGE_SELF)
and `:136` (`proc_pid_rusage` with `rusage_info_v2` on macOS, `/proc/self/io`
on Linux, `None` elsewhere). Copy that code into this crate behind a feature,
keep its platform arms, credit nothing in comments.

## Candidates to price

Each is a separate cargo feature, off by default, named in the table. Add none
that this brief does not list; propose additions in the receipts doc instead.

| feature | crate | what it gives |
|---|---|---|
| `fmt` | `tracing-subscriber` fmt layer | the text-log baseline, already present |
| `chrome` | `tracing-chrome` | already present |
| `otlp-trace` | `opentelemetry-otlp` trace | already present, today's `otlp` |
| `otlp-metrics` | add `metrics` to `opentelemetry_sdk` and `-otlp` | OTel metrics pipeline |
| `sysmetrics` | `opentelemetry-system-metrics` 0.4 | CPU, memory, disk, network; pairs with the otel 0.32 already pinned; needs tokio and sysinfo |
| `procmetrics` | `metrics-process` 2.4.3 | CPU user and system, RSS, vmem, fds, OS thread count, start time; Linux, macOS, Windows, FreeBSD |
| `metrics-ctx` | `metrics-tracing-context` | `MetricsLayer`, a real `tracing_subscriber::Layer` turning span fields into metric labels |
| `tracy` | `tracing-tracy` | sampled callstacks, gated behind its own `enable` feature |
| `tracy-alloc` | `tracy_full` | `#[global_allocator]` allocation tracking |
| `rusage` | the lifted `proc_pid_rusage` code | per-process CPU, RSS, disk read and write bytes |
| `sqlite-sink` | existing `sqlite` | the relational sink, see below |

Known caveat to state in the receipts, not to discover: the Tracy layer drops
spans entered and exited on different threads, so its numbers are wrong under
async. Say so in the table's notes column.

## Flush strategies

Every sink is measured under all three. This is an enum in the crate's public
`Config`, not a per-sink hack.

| strategy | when the sink writes |
|---|---|
| `immediate` | on every event, inline on the emitting thread |
| `drain` | events buffered, flushed by a background drain at a bounded interval or bounded buffer size, whichever first |
| `on-commit` | buffered, flushed when the host declares a commit point |

The bound on the drain buffer is a constant with a comment saying what it
protects, and the drain stops with a named diagnostic when the bound is hit.
Every loop and every recursion in this crate is bounded; a scanner test lists
every `loop {` against its budget line.

## The relational sink

The user's rule, and the reason this lab exists: log records are relational
data, and writing repeated strings into rows is denormalization. Field names,
span names, targets, file paths and levels repeat across nearly every event;
the row identity does not. So the sqlite sink dictionary-encodes the repeating
columns into side tables with surrogate INTEGER keys and joins, and stores the
per-row values as they are.

Read `sql-relational-design` and `sqlite-costs` before writing any DDL.
Surrogate INTEGER keys; natural TEXT keys once in a dictionary table; never a
composite TEXT primary key.

A measured rule from the sibling repo constrains the design and is not up for
rediscovery: interning pays on a column whose key repeats across many rows, and
loses on a column that is one identity per row. Apply it. Measure both the
dictionary-encoded sink and a naive all-TEXT sink, and report both rows. If the
dictionary loses at this scale, say so with the three runs that show it.

## Differential measurement: the whole point

Every number is a difference between the same binary with a layer on and with
it off. A single absolute number is not a receipt.

For each candidate feature M:

1. Build the harness with M off. Build it with M on. Same profile, same
   `CARGO_TARGET_DIR`, release.
2. Run the workload three times each side. Three runs, never one.
3. Report the percent cost as the change in the median, and print all six raw
   numbers beside it.
4. A candidate lands in the table as a cost only if all three on-runs are
   outside the spread of all three off-runs. Otherwise the cell reads `in the
   noise` and shows the six numbers anyway.

Logging that is on during a timing run corrupts that run; this repo family has
already lost a real gain that way. The off side means genuinely off, not
filtered to `error`.

## The table

One `just` recipe prints it. Name the recipe `watch-the-watchman`. It builds,
measures, and writes both a terminal table and a `.tsv` beside the receipts
doc. It takes under ten minutes total or it prints a partial table and says
which cells it skipped; no single operation inside it runs over ten seconds
without a progress line.

Columns, one row per candidate feature per flush strategy:

| column | source |
|---|---|
| `feature` | the cargo feature |
| `strategy` | immediate, drain, on-commit |
| `crates_added` | `cargo tree -e normal` node count, on minus off |
| `binary_bytes` | stripped release artifact, on minus off |
| `build_secs` | clean build wall, on minus off, three runs |
| `peak_rss_bytes` | the lifted `proc_pid_rusage` sampler |
| `disk_write_bytes` | same sampler |
| `disk_read_bytes` | same sampler |
| `wall_ms_off` | three raw numbers |
| `wall_ms_on` | three raw numbers |
| `pct_cost` | median on over median off, minus one |
| `events_per_sec` | throughput at the workload's event rate |
| `verdict` | `cost` or `in the noise` |
| `notes` | the Tracy async caveat and anything like it |

## The workload

One workload, shared by every row, defined in the harness: a fixed event rate
and span shape that looks like real engine work, with nested spans, a hot inner
span, and a field set that repeats its names. Its parameters are constants in
one place. It is deterministic; there is no RNG. State that plainly in the
receipts, because a reader will ask about seeds.

## Receipts

- R1: every candidate feature compiles alone and with `--all-features`.
- R2: the table, complete, from the one `just` command, with the `.tsv`.
- R3: the three-run raw numbers for every cell, not only the medians.
- R4: the dictionary sink and the all-TEXT sink measured against each other.
- R5: `cargo clippy --all-targets -D warnings` clean on owned paths.
- R6: the pre-existing red legs of this workspace, recorded before your first
  edit and unchanged after.
- R7: `git diff --stat origin/main...HEAD` lists only owned files.
- R8: the bounded-loop scanner test passes.
- R9: the plain-words twin, with row tables and one-line-per-edge trees, no
  mermaid.

## Laws

- Build-vs-buy: every library here is bought. Write no bespoke sampler,
  queue, scheduler, retry, or telemetry format. If a candidate needs glue,
  the glue is a layer, not a framework.
- Doubt yourself before asserting. Verify against the code. Comments are not
  the language.
- `eprintln!` never in `src/**`; `tracing` only. A rare CLI-UX line carries
  `@eprintln-ok`.
- Comment budget: only constraints the code cannot show. No change-log
  narrative, no dates, no arc references.
- Banned words, prose and identifiers: provenance, substrate, load-bearing,
  regime, "ground truth" (say oracle), "support" (say refCount). No em dashes.
- Textbook register: short sentences, present tense, no "we". No stray numbers
  in sentences.
- Inside a file, follow that file's style.
- Nothing seizes the machine. A change that can beachball it is a blocking
  defect. Give the bench its own `CARGO_TARGET_DIR`.
- Measurement logs go under the lane's `CARGO_TARGET_DIR`, never `/tmp`.
- Every incident that bites gets a row in the repo's failure-modes doc.

## Working rules

- Commit at every arm that compiles, before measuring. A lane that exits
  uncommitted loses its work; this has happened twice in the sibling repo.
- Work in `$PWD`, your worktree. Never `cd` to the primary checkout.
- Report with `boop beep --no-wait --as <lane> sprefa-coordinator "<one line>"`
  at each receipt.
- Spawn no subagents.
- If a candidate cannot be made to work, that is a row in the table with the
  throw site cited, not a silent omission.
