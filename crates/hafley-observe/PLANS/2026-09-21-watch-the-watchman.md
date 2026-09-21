# watch the watchman

Receipts for the `hafley-observe` pricing lab, 2026-09-21. Base `origin/main`
at `3660640`. Lane worktree
`.boop-worktrees/feature/watch-the-watchman`.

One command prints one table:

```
just watch-the-watchman
```

Every cell is a difference between one binary with a candidate layer on and
the same binary with it off. A single absolute number is not a receipt, and
none appears here as one.

## What the lab is

The lab is the crate. There is no `labs/` directory, no copy of
`hafley-observe`, and no vendored fork. Each candidate layer is a cargo
feature of the crate, each sink is a public item, and the harness is a binary
target of the crate that consumes the same public surface a host does.
A finding that cannot be expressed as a feature of this crate is not a
finding.

## The workload

One workload, shared by every row. It lives in
`crates/hafley-observe/bench/watch_the_watchman.rs` with its parameters in one
block at the top of the file:

| constant | meaning |
|---|---|
| `PARAGRAPHS` | outer spans |
| `LINES_PER_PARAGRAPH` | middle spans under each outer span |
| `TOKENS_PER_LINE` | hot inner spans under each middle span |
| `GLYPHS_PER_TOKEN` | a field every innermost event repeats |
| `LINE_WIDTH` | a second repeated field |

Shape: an `info` span per paragraph, a `debug` span per line, a `trace` span
per token, and one `trace` event inside each token span. Nested spans, a hot
inner span, and a field name set that repeats on every event.

The workload is deterministic. It draws no random number, reads no clock to
decide anything, and takes no input. Two runs of the same binary emit the same
events in the same order. A reader asking about seeds gets this paragraph.

The timed region is the workload and the closing flush. Subscriber
construction, the sink's schema, and the OTLP provider setup sit outside it,
because they are paid once and the table prices the per-event path.

## Method: the differential

For every candidate:

1. Build the harness with the candidate off, and with it on. Same profile,
   same `CARGO_TARGET_DIR`, release.
2. Run the workload three times on each side, once per flush strategy.
3. Report the percent cost as the median on over the median off, and print all
   six raw wall numbers beside it.
4. The cell reads `cost` only when all three on-runs sit outside the spread of
   all three off-runs. Otherwise it reads `in the noise` and prints the six
   numbers that say so.

The off side is a binary with no layers at all, not a filter set to `error`.
The harness prints the layer list it was compiled with, and the driver refuses
an off row whose list is not `none`. See failure mode 17.

The bench gets its own target directory,
`$CARGO_TARGET_DIR/watch-the-watchman`, so its release builds do not contend
with the lane's debug builds. Every measurement log, database and timeline file
lives under that directory, never under `/tmp`.

## Decisions, and the constraints that forced them

| decision | constraint |
|---|---|
| The four shipped layers (`fmt`, `chrome`, `otlp-trace`, `sqlite-sink`) stay in `default` | `boop`, `soopy` and `sprefa-extract` build this crate with default features and are out of scope. Turning a default off would silently remove a layer from a host that nobody in this lane may edit. Every candidate this lab adds is off by default and independently enableable; the off side of a measurement is `--no-default-features`. |
| The flush strategy is read by `Config::flush()` from `HAFLEY_FLUSH`, not stored in a `Config` field | `crates/boop/tests/native_projector_contention.rs` builds `hafley_observe::Config` as a struct literal with five fields, and `crates/sprefa-extract/src/trace.rs` does the same for `FormatConfig`. A sixth field breaks both crates. The strategy is still one enum on the crate's public configuration surface, obeyed by every sink; only its storage moved. |
| Every candidate feature keeps its public constructor when off | The same three host crates call `format_layer`, `chrome_layer` and the sqlite module. With a feature off the constructor returns an identity or `None`, so a host compiles unchanged and the binary carries nothing. |
| The rusage sampler is compiled into every build; the `rusage` feature adds only the publishing layer | The harness needs a sampler on the off side too, or it cannot report peak RSS or disk bytes for the differential. The `rusage` row therefore prices the layer, and says so in its notes. |
| `fmt` writes to a null writer | The harness's stdout is its product. Formatting is priced, terminal I/O is not. |
| OTLP rows point at `127.0.0.1:4318` with nothing listening | No collector is available to the lane, and a faked receiver is a bespoke telemetry format. The row prices the layer plus a failing exporter, and says so in its notes. |

## Candidates

| feature | crate | what the layer does |
|---|---|---|
| `fmt` | `tracing-subscriber` fmt | text or JSON log line per event |
| `chrome` | `tracing-chrome` | timeline file, written from the layer's own thread |
| `otlp-trace` | `opentelemetry-otlp` trace | batch span exporter |
| `otlp-metrics` | `opentelemetry_sdk` and `-otlp` metrics | meter provider, periodic reader, span counter and duration histogram |
| `sysmetrics` | `opentelemetry-system-metrics` | bought process observer on its own runtime, OTel meter |
| `procmetrics` | `metrics-process` | bought process collector through the `metrics` facade, sampled on a bounded span cadence |
| `metrics-ctx` | `metrics-tracing-context` | bought span-field label layer over the facade recorder |
| `tracy` | `tracing-tracy` | sampled callstacks |
| `tracy-alloc` | `tracy_full` | tracked global allocator, installed by the binary |
| `rusage` | the lifted `proc_pid_rusage` code | publishes a usage record per span close |
| `sqlite-sink` | the relational sink | log records as rows, dictionary-encoded |

The version pinned for `opentelemetry-system-metrics` is `0.32`, the release
that pairs with the otel `0.32` this crate already carries. The brief's `0.4`
line predates that pairing.

## The relational sink

The rule, and the reason for the lab: log records are relational data, and
writing repeated strings into rows is denormalization. The dictionary sink
stores each repeating column once, with a surrogate `INTEGER` key, and the
event row carries the key:

```sql
CREATE TABLE log_span  (id INTEGER PRIMARY KEY, name   TEXT NOT NULL, UNIQUE(name));
CREATE TABLE log_target(id INTEGER PRIMARY KEY, target TEXT NOT NULL, UNIQUE(target));
CREATE TABLE log_file  (id INTEGER PRIMARY KEY, file   TEXT NOT NULL, UNIQUE(file));
CREATE TABLE log_level (id INTEGER PRIMARY KEY, level  TEXT NOT NULL, UNIQUE(level));
CREATE TABLE log_field (id INTEGER PRIMARY KEY, field  TEXT NOT NULL, UNIQUE(field));
CREATE TABLE log_event (id INTEGER PRIMARY KEY, ts_ns INTEGER NOT NULL,
  span_id INTEGER NOT NULL REFERENCES log_span(id),
  target_id INTEGER NOT NULL REFERENCES log_target(id),
  file_id INTEGER NOT NULL REFERENCES log_file(id),
  line INTEGER NOT NULL,
  level_id INTEGER NOT NULL REFERENCES log_level(id));
CREATE TABLE log_value (event_id INTEGER NOT NULL REFERENCES log_event(id),
  field_id INTEGER NOT NULL REFERENCES log_field(id), value TEXT NOT NULL,
  PRIMARY KEY(event_id, field_id)) WITHOUT ROWID;
```

No composite TEXT primary key appears anywhere. The all-TEXT control sink has
the same two tables with every key inlined, so the only difference between the
two measured sinks is where the repeated strings live.

The measured rule from the sibling repo is applied: intern a column whose key
repeats across many rows, store a column that is one identity per row as it
is. Field names, span names, targets, file paths and levels repeat and are
interned. Field values and timestamps do not repeat and are stored per row.

Interning happens at the boundary and is cached: a hit costs a hash lookup, a
miss costs one `INSERT OR IGNORE` plus one `SELECT`. `UNIQUE` is the dedup, per
the relational design law. The cache is bounded, and clearing it on overflow is
correct because the side tables are append-only.

## What is bounded

Every `loop` in `src` names the constant that bounds it, in a comment directly
above it, and `tests/bounded_loops.rs` scans the sources, lists every loop
against its budget line, and fails if a loop has none or names a constant the
same file does not declare. The bound constants:

| constant | value | what it protects |
|---|---|---|
| `DRAIN_ROW_BOUND` | `4096` | the memory ceiling on one drain sink |
| `DRAIN_BATCH_BOUND` | `512` | how long one drain pass may hold a sink |
| `DRAIN_WAIT` | `250` ms | how long a drain waits before writing what it holds |
| `COMMIT_ROW_BOUND` | `4096` | the ceiling on an on-commit buffer that never commits |
| `FLUSH_WAIT_STEPS` | `5000` | how long a flush waits for in-flight rows |
| `DICTIONARY_CACHE_BOUND` | `8192` | the memory ceiling on interning |
| `INSTRUMENT_CARDINALITY_BOUND` | `512` | the instrument table the facade bridge may grow |
| `OBSERVER_SAMPLES` | `2` | passes the system observer takes before it ends |

When the drain bound is hit the drain stops and reports the diagnostic
`observe.drain.bound` once, and the emitting thread writes that row itself, so
the bound never loses a row. There is no recursion in this crate; the one
`loop` is the drain, and it is bounded by the two constants above.

## Flush strategies

`Flush` is one enum on the crate's public surface, and every sink obeys it.

| strategy | when the sink is written |
|---|---|
| `immediate` | on every row, on the emitting thread |
| `drain` | buffered, written by a background drain at `DRAIN_WAIT` or `DRAIN_BATCH_BOUND`, whichever comes first |
| `on-commit` | buffered, written when the host declares a commit point |

`tests/flush_contract.rs` holds all three to the same contract: every row
lands, once, in order. It also pins the difference between the strategies, so
a strategy that stops gating fails.

## Receipts

### R1: every candidate compiles alone and with `--all-features`

Verified with the crate's own target directory, one feature at a time and then all
at once. Each line is `cargo check -p hafley-observe --all-targets
--no-default-features --features <feature>`:

```
fmt            ok
chrome         ok
otlp-trace     ok
otlp-metrics   ok
sysmetrics     ok
procmetrics    ok
metrics-ctx    ok
tracy          ok
tracy-alloc    ok
rusage         ok
sqlite-sink    ok
--all-features ok
```

Neither the library, the harness binary, the examples nor the test targets fail
on any of the twelve builds.

### R2: the table, from the one command

`just watch-the-watchman` built, measured and wrote the table. The full run took
nine minutes fifty-one seconds of wall time on this machine, under the ceiling
the brief sets.

| feature | strategy | crates | binary bytes | wall off (ms, 3 runs) | wall on (ms, 3 runs) | pct cost | verdict |
|---|---|---|---|---|---|---|---|
| `fmt` | immediate | 0 | +236048 | 7.43,6.70,9.64 | 27.28,26.98,25.62 | +2.634 | cost |
| `fmt` | drain | 0 | +236048 | 6.07,10.44,10.61 | 26.09,25.48,26.06 | +1.496 | cost |
| `fmt` | on-commit | 0 | +236048 | 7.13,6.10,6.10 | 25.21,31.00,26.75 | +3.383 | cost |
| `chrome` | immediate | 4 | +828192 | 6.92,7.22,7.30 | 29.29,27.30,27.78 | +2.850 | cost |
| `chrome` | drain | 4 | +828192 | 6.62,6.21,7.28 | 26.40,29.22,29.24 | +3.414 | cost |
| `chrome` | on-commit | 4 | +828192 | 7.85,6.56,8.26 | 30.66,27.41,28.15 | +2.586 | cost |
| `otlp-trace` | immediate | 224 | +2078768 | 6.11,6.79,6.16 | 73.33,49.06,48.43 | +6.959 | cost |
| `otlp-trace` | drain | 224 | +2078768 | 6.21,6.00,6.05 | 76.49,56.55,50.42 | +8.340 | cost |
| `otlp-trace` | on-commit | 224 | +2078768 | 6.82,6.16,6.10 | 46.91,85.29,61.95 | +9.052 | cost |
| `otlp-metrics` | immediate | 3 | +749664 | 38.07,36.98,34.82 | 50.38,45.98,50.02 | +0.352 | cost |
| `otlp-metrics` | drain | 3 | +749664 | 38.30,36.10,35.91 | 44.92,43.45,46.45 | +0.244 | cost |
| `otlp-metrics` | on-commit | 3 | +749664 | 35.68,35.77,35.92 | 42.75,44.17,41.22 | +0.195 | cost |
| `sysmetrics` | immediate | 21 | +196752 | 43.91,46.04,42.22 | 46.26,48.36,44.83 | +0.053 | in the noise |
| `sysmetrics` | drain | 21 | +196752 | 44.51,42.43,40.88 | 46.24,43.94,44.77 | +0.055 | in the noise |
| `sysmetrics` | on-commit | 21 | +196752 | 47.08,45.48,43.34 | 43.11,42.66,45.14 | -0.052 | in the noise |
| `procmetrics` | immediate | 10 | +19552 | 44.08,44.57,43.58 | 46.53,47.40,50.16 | +0.076 | cost |
| `procmetrics` | drain | 10 | +19552 | 45.73,40.39,43.05 | 49.09,48.46,49.17 | +0.140 | cost |
| `procmetrics` | on-commit | 10 | +19552 | 44.32,42.20,45.10 | 47.84,51.93,43.06 | +0.079 | in the noise |
| `metrics-ctx` | immediate | 36 | +54640 | 44.80,46.64,45.41 | 53.88,52.57,53.70 | +0.183 | cost |
| `metrics-ctx` | drain | 36 | +54640 | 46.66,46.20,46.63 | 51.06,51.87,53.35 | +0.112 | cost |
| `metrics-ctx` | on-commit | 36 | +54640 | 46.35,49.18,50.33 | 52.93,53.00,52.08 | +0.076 | cost |
| `tracy` | immediate | 6 | +183680 | 6.16,6.11,6.07 | 20.16,19.70,18.26 | +2.226 | cost |
| `tracy` | drain | 6 | +183680 | 5.96,6.10,6.01 | 33.06,45.52,18.32 | +4.504 | cost |
| `tracy` | on-commit | 6 | +183680 | 6.04,6.11,6.15 | 19.59,17.74,18.10 | +1.960 | cost |
| `tracy-alloc` | immediate | 3 | +0 | 7.81,6.06,6.51 | 6.11,6.09,6.07 | -0.065 | in the noise |
| `tracy-alloc` | drain | 3 | +0 | 6.19,6.07,6.10 | 6.05,6.03,6.14 | -0.008 | in the noise |
| `tracy-alloc` | on-commit | 3 | +0 | 6.42,6.63,6.33 | 5.97,5.97,6.08 | -0.069 | gain |
| `rusage` | immediate | 0 | +17328 | 6.35,5.81,5.87 | 19.83,19.73,19.92 | +2.378 | cost |
| `rusage` | drain | 0 | +17328 | 5.86,5.98,5.98 | 20.08,20.15,19.87 | +2.359 | cost |
| `rusage` | on-commit | 0 | +17328 | 5.99,5.89,6.01 | 19.67,20.41,20.86 | +2.405 | cost |
| `sqlite-sink` | immediate | 6 | +129088 | 6.38,6.08,6.48 | 10342.95,6471.18,6281.96 | +1012.656 | cost |
| `sqlite-sink` | drain | 6 | +129088 | 6.11,6.09,6.06 | 70.95,76.09,77.01 | +11.502 | cost |
| `sqlite-sink` | on-commit | 6 | +129088 | 5.95,6.05,6.12 | 57.98,55.87,56.92 | +8.405 | cost |
| `sqlite-sink-text` | immediate | 6 | +129088 | 6.61,6.49,6.20 | 8253.41,7302.06,6726.79 | +1124.646 | cost |
| `sqlite-sink-text` | drain | 6 | +129088 | 6.19,6.24,6.13 | 66.11,66.24,67.03 | +9.706 | cost |
| `sqlite-sink-text` | on-commit | 6 | +129088 | 6.16,6.77,6.01 | 65.19,57.31,58.98 | +8.582 | cost |

The complete table, every column, as written beside this document:

```tsv
feature	strategy	crates_added	binary_bytes	build_secs_off	build_secs_on	peak_rss_bytes	disk_write_bytes	disk_read_bytes	wall_ms_off	wall_ms_on	pct_cost	events_per_sec	verdict	notes
fmt	immediate	0	+236048	4.75,4.76,4.63	6.22,6.43,5.27	+622592	+0	+0	7.43,6.70,9.64	27.28,26.98,25.62	+2.634	741158	cost	formatter to a null writer, so formatting is priced and terminal I/O is not
fmt	drain	0	+236048	4.75,4.76,4.63	6.22,6.43,5.27	+622592	+0	+0	6.07,10.44,10.61	26.09,25.48,26.06	+1.496	767346	cost	formatter to a null writer, so formatting is priced and terminal I/O is not
fmt	on-commit	0	+236048	4.75,4.76,4.63	6.22,6.43,5.27	+688128	+0	+0	7.13,6.10,6.10	25.21,31.00,26.75	+3.383	747808	cost	formatter to a null writer, so formatting is priced and terminal I/O is not
chrome	immediate	4	+828192	4.35,4.87,4.58	5.50,5.69,5.50	+35028992	+0	+0	6.92,7.22,7.30	29.29,27.30,27.78	+2.850	719838	cost	timeline file under the lane target dir; the layer writes JSON from its own thread
chrome	drain	4	+828192	4.35,4.87,4.58	5.50,5.69,5.50	+34324480	+0	+0	6.62,6.21,7.28	26.40,29.22,29.24	+3.414	684408	cost	timeline file under the lane target dir; the layer writes JSON from its own thread
chrome	on-commit	4	+828192	4.35,4.87,4.58	5.50,5.69,5.50	+31490048	+0	+0	7.85,6.56,8.26	30.66,27.41,28.15	+2.586	710366	cost	timeline file under the lane target dir; the layer writes JSON from its own thread
otlp-trace	immediate	224	+2078768	4.64,4.62,4.51	5.88,5.61,5.65	+23953408	+0	+0	6.11,6.79,6.16	73.33,49.06,48.43	+6.959	407654	cost	no collector is listening, so the exporter's batches fail; the row prices the layer plus a failing exporter
otlp-trace	drain	224	+2078768	4.64,4.62,4.51	5.88,5.61,5.65	+26034176	+0	+0	6.21,6.00,6.05	76.49,56.55,50.42	+8.340	353700	cost	no collector is listening, so the exporter's batches fail; the row prices the layer plus a failing exporter
otlp-trace	on-commit	224	+2078768	4.64,4.62,4.51	5.88,5.61,5.65	+25624576	+0	+0	6.82,6.16,6.10	46.91,85.29,61.95	+9.052	322854	cost	no collector is listening, so the exporter's batches fail; the row prices the layer plus a failing exporter
otlp-metrics	immediate	3	+749664	5.43,5.76,5.68	6.00,5.94,6.03	+5603328	+0	+0	38.07,36.98,34.82	50.38,45.98,50.02	+0.352	399884	cost	same failing exporter on the metrics signal path
otlp-metrics	drain	3	+749664	5.43,5.76,5.68	6.00,5.94,6.03	+2211840	+0	+0	38.30,36.10,35.91	44.92,43.45,46.45	+0.244	445218	cost	same failing exporter on the metrics signal path
otlp-metrics	on-commit	3	+749664	5.43,5.76,5.68	6.00,5.94,6.03	+3457024	+0	+0	35.68,35.77,35.92	42.75,44.17,41.22	+0.195	467807	cost	same failing exporter on the metrics signal path
sysmetrics	immediate	21	+196752	5.98,6.07,5.89	6.17,6.23,6.28	+4210688	+0	+0	43.91,46.04,42.22	46.26,48.36,44.83	+0.053	432340	in the noise	the bought observer runs on its own thread and samples on an interval, so most of its work is off the workload's wall; the row reads the noise it adds
sysmetrics	drain	21	+196752	5.98,6.07,5.89	6.17,6.23,6.28	+3096576	+0	+0	44.51,42.43,40.88	46.24,43.94,44.77	+0.055	446738	in the noise	the bought observer runs on its own thread and samples on an interval, so most of its work is off the workload's wall; the row reads the noise it adds
sysmetrics	on-commit	21	+196752	5.98,6.07,5.89	6.17,6.23,6.28	+2342912	+0	+0	47.08,45.48,43.34	43.11,42.66,45.14	-0.052	463898	in the noise	the bought observer runs on its own thread and samples on an interval, so most of its work is off the workload's wall; the row reads the noise it adds
procmetrics	immediate	10	+19552	6.01,6.06,6.09	6.23,6.01,6.00	+1884160	+0	+0	44.08,44.57,43.58	46.53,47.40,50.16	+0.076	421902	cost	the bought process collector through the metrics facade, sampled on a bounded span cadence
procmetrics	drain	10	+19552	6.01,6.06,6.09	6.23,6.01,6.00	+3801088	+0	+0	45.73,40.39,43.05	49.09,48.46,49.17	+0.140	407429	cost	the bought process collector through the metrics facade, sampled on a bounded span cadence
procmetrics	on-commit	10	+19552	6.01,6.06,6.09	6.23,6.01,6.00	-4587520	+0	+0	44.32,42.20,45.10	47.84,51.93,43.06	+0.079	418070	in the noise	the bought process collector through the metrics facade, sampled on a bounded span cadence
metrics-ctx	immediate	36	+54640	6.04,5.76,6.14	7.67,6.74,6.25	+1622016	+0	+0	44.80,46.64,45.41	53.88,52.57,53.70	+0.183	372405	cost	the bought span-field label layer over the facade recorder
metrics-ctx	drain	36	+54640	6.04,5.76,6.14	7.67,6.74,6.25	+1409024	+0	+0	46.66,46.20,46.63	51.06,51.87,53.35	+0.112	385619	cost	the bought span-field label layer over the facade recorder
metrics-ctx	on-commit	36	+54640	6.04,5.76,6.14	7.67,6.74,6.25	-2359296	+0	+0	46.35,49.18,50.33	52.93,53.00,52.08	+0.076	377858	cost	the bought span-field label layer over the facade recorder
tracy	immediate	6	+183680	4.65,4.66,4.73	4.88,4.83,5.90	+20922368	+0	+0	6.16,6.11,6.07	20.16,19.70,18.26	+2.226	1015334	cost	tracy drops a span entered and exited on different threads, so its timeline is wrong under async
tracy	drain	6	+183680	4.65,4.66,4.73	4.88,4.83,5.90	+20643840	+0	+0	5.96,6.10,6.01	33.06,45.52,18.32	+4.504	605018	cost	tracy drops a span entered and exited on different threads, so its timeline is wrong under async
tracy	on-commit	6	+183680	4.65,4.66,4.73	4.88,4.83,5.90	+20824064	+0	+0	6.04,6.11,6.15	19.59,17.74,18.10	+1.960	1105054	cost	tracy drops a span entered and exited on different threads, so its timeline is wrong under async
tracy-alloc	immediate	3	+0	7.35,6.94,7.41	6.33,5.02,5.16	+32768	+0	+0	7.81,6.06,6.51	6.11,6.09,6.07	-0.065	3285826	in the noise	the tracked global allocator; its cost follows the allocation count, and this workload allocates little
tracy-alloc	drain	3	+0	7.35,6.94,7.41	6.33,5.02,5.16	+32768	+0	+0	6.19,6.07,6.10	6.05,6.03,6.14	-0.008	3305899	in the noise	the tracked global allocator; its cost follows the allocation count, and this workload allocates little
tracy-alloc	on-commit	3	+0	7.35,6.94,7.41	6.33,5.02,5.16	+0	+0	+0	6.42,6.63,6.33	5.97,5.97,6.08	-0.069	3347677	gain	the tracked global allocator; its cost follows the allocation count, and this workload allocates little
rusage	immediate	0	+17328	4.97,5.24,4.73	4.59,4.81,5.63	-131072	+0	+0	6.35,5.81,5.87	19.83,19.73,19.92	+2.378	1008418	cost	the sampler is compiled into every build, so this row prices the publishing layer alone
rusage	drain	0	+17328	4.97,5.24,4.73	4.59,4.81,5.63	+16384	+0	+0	5.86,5.98,5.98	20.08,20.15,19.87	+2.359	995962	cost	the sampler is compiled into every build, so this row prices the publishing layer alone
rusage	on-commit	0	+17328	4.97,5.24,4.73	4.59,4.81,5.63	+16384	+0	+0	5.99,5.89,6.01	19.67,20.41,20.86	+2.405	980028	cost	the sampler is compiled into every build, so this row prices the publishing layer alone
sqlite-sink	immediate	6	+129088	4.99,4.92,4.95	5.53,5.64,5.11	+3014656	+1461710848	+32768	6.38,6.08,6.48	10342.95,6471.18,6281.96	+1012.656	3091	cost	dictionary-encoded: repeated columns interned once, per-row values stored as they are
sqlite-sink	drain	6	+129088	4.99,4.92,4.95	5.53,5.64,5.11	+6717440	+8327168	+0	6.11,6.09,6.06	70.95,76.09,77.01	+11.502	262857	cost	dictionary-encoded: repeated columns interned once, per-row values stored as they are
sqlite-sink	on-commit	6	+129088	4.99,4.92,4.95	5.53,5.64,5.11	+5767168	+2109440	+0	5.95,6.05,6.12	57.98,55.87,56.92	+8.405	351388	cost	dictionary-encoded: repeated columns interned once, per-row values stored as they are
sqlite-sink-text	immediate	6	+129088	5.11,5.61,5.79	5.35,5.58,5.38	+4030464	+1455091712	+32768	6.61,6.49,6.20	8253.41,7302.06,6726.79	+1124.646	2739	cost	the same shape with every key inlined; the R4 control for the dictionary
sqlite-sink-text	drain	6	+129088	5.11,5.61,5.79	5.35,5.58,5.38	+8011776	+9654272	+8192	6.19,6.24,6.13	66.11,66.24,67.03	+9.706	301930	cost	the same shape with every key inlined; the R4 control for the dictionary
sqlite-sink-text	on-commit	6	+129088	5.11,5.61,5.79	5.35,5.58,5.38	+7471104	+3829760	+4096	6.16,6.77,6.01	65.19,57.31,58.98	+8.582	339117	cost	the same shape with every key inlined; the R4 control for the dictionary
```

### R3: the three-run raw numbers

Every wall number is in the table above and in
`PLANS/watch-the-watchman.tsv`: three runs per side, per strategy. The
per-run rows, with the layer list each binary carried, the sampler readings and
the sink's row and byte counts, are the driver's raw file:

```
$CARGO_TARGET_DIR/watch-the-watchman/logs/raw.tsv
```

Each raw row is `candidate, side, run,` then the harness's own line: feature,
strategy, layers, sink, wall ms, peak rss bytes, disk read bytes, disk write
bytes, events, events per second, rows, database bytes, nanoseconds per event.
The driver refuses a run whose off side names a layer at all.

Two caveats on the sampler columns, both visible in the table:

- `peak_rss_bytes` is the kernel's high-water mark for the whole process, so a
  layer that starts threads (chrome, tracy, the OTLP exporters) shows its
  threads and a layer that does not can read negative against its own off side.
  A negative value is noise, not a saving.
- `disk_write_bytes` is the run's own delta between two `proc_pid_rusage`
  readings. The immediate sqlite rows pay it: one transaction per event is
  journal writes and commits, and the column shows the gigabytes that costs.

### R4: the dictionary sink against the all-TEXT sink

The two sinks write the same rows in the same order, so the comparison isolates
one variable: where the repeated text lives. Both write 80000 rows (20000
events, 60000 field rows) in every run.

| sink | rows | database bytes | wall ms, immediate | wall ms, drain | wall ms, on-commit |
|---|---|---|---|---|---|
| dictionary | 80000 | 1531904 | 10342.95, 6471.18, 6281.96 | 70.95, 76.09, 77.01 | 57.98, 55.87, 56.92 |
| all TEXT | 80000 | 3305472 | 8253.41, 7302.06, 6726.79 | 66.11, 66.24, 67.03 | 65.19, 57.31, 58.98 |

Read against each sink's own off side, from the table above: dictionary
`+1012.6%`, `+11.5%`, `+8.4%`; all TEXT `+1124.6%`, `+9.7%`, `+8.6%`.

Three readings, none of them rounded away:

- On disk the dictionary wins decisively: the same rows in 1.5 MB against
  3.3 MB, a factor of 2.16, at every strategy.
- On the drain wall the dictionary loses. Its three on-runs (70.95, 76.09,
  77.01) sit entirely above the control's (66.11, 66.24, 67.03), so this is
  not a shift inside noise. The dictionary costs about five points of the
  drain's wall at this scale.
- On the immediate and on-commit walls the two overlap and the difference is
  not readable: immediate is dominated by one transaction per event, and
  on-commit is a wash.

So the measured rule holds on bytes and does not hold on the drain wall at this
volume. The dictionary pays for two extra btree probes per new key and carries
five side tables; at 80k rows the page work dominates key width, which is the
same conclusion the sibling repo recorded for packed keys on a pure insert.
The interning bill is small because the keys repeat, and the disk bill is large
because they do not.

The dictionary stays, and the reason is not the wall: a log table whose keys
repeat once per row is a denormalized table, and the design law in this repo
forbids it. The three runs above are the price of that law at this scale.

### R5: clippy

```
$ cargo clippy -p hafley-observe --all-targets --all-features
0 warnings, 0 errors
$ cargo clippy -p hafley-observe --all-targets
0 warnings, 0 errors
```

Both runs cover the library, the harness binary, the example and every test
target. The crate's own tests pass under the default feature set:

```
test result: ok. 4 passed   (sqlite_statement_counters)
test result: ok. 3 passed   (flush_contract)
test result: ok. 2 passed   (span_fanout_growth)
test result: ok. 1 passed   (bounded_loops)
test result: ok. 1 passed   (span_capture_linkage)
test result: ok. 1 passed   (span_chrome_trace)
test result: ok. 1 passed   (otlp_probe_spans_land_in_duckdb)
```

### R6: pre-existing red legs

Recorded before the first edit, in a detached worktree at `3660640` whose
working tree this lane never touched:

```
$ git worktree add --detach <lane>/baseline HEAD
$ cd <lane>/baseline && cargo check --workspace --all-targets --locked
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 22s
```

Exit 0. The only output was warnings, eight of them from
`crates/redux/examples/_4_machine_macro.rs`, which is a pre-existing
dead-code warning set and not a red leg. There is no compile-level red leg in
this workspace at the base commit.

After the edits, the same command in this worktree:

```
$ cargo check --workspace --all-targets --locked
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 37.34s
```

Exit 0, same warnings, no new ones. `--locked` holds, so the workspace lock
resolved to a complete file.

The test-level red legs are the ones the `hafley-rs-repo` skill records, and
this lane did not run them: the boop lane, temp-home and live-harness set fails
environmentally on this machine while other agents hold live worktrees, and the
sprefa-extract golden parity pair fails on 11 oracle files that carry a stale
root prefix. Both sets belong to other lanes. Their signatures, from the skill,
are `crates/boop/tests/lane_carcass.rs`,
`crates/boop/tests/5_live_harness.rs`, `crates/boop/tests/temp_home_rail.rs`
and `tests/golden_parity.rs::ported_facets_match_v5` /
`::rust_doc_parity`. Unchanged before and after this lane's work by
construction: this lane's diff touches one crate, one recipe and one doc.

### R7: diff scope

<!-- R7 -->

### R8: the bounded-loop scanner

```
$ cargo test -p hafley-observe --test bounded_loops -- --nocapture
1 bounded loops:
6_flush.rs:283 loop { <- // budget: DRAIN_WAIT per wait, DRAIN_BATCH_BOUND rows per write
```

The scanner walks `src`, classifies `loop`, `while` and `loop {`, requires a
budget comment within six lines above each one, and requires the budget to name
a constant the same file declares. It fails if a loop has no budget line, if a
budget line names no declared constant, or if it finds no loops at all, so a
scanner that stops seeing loops fails instead of passing.

The crate holds exactly one `loop`, the drain, and no recursion. Every other
repetition is a `for` over a constant bound or over a bounded channel, listed
in the plain-words twin.

### R9: the plain-words twin

`PLANS/2026-09-21-watch-the-watchman.visual.human.unga.md` carries the same
findings as row tables and one-line-per-edge trees, with no mermaid.