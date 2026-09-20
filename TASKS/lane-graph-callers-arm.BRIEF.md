# Lane: `ryi graph --callers NAME`, first graph arm plus shared `GraphCx`

Repo `~/projects/hafley-rs`, crate `crates/sprefa-extract` (binary `ryi`).
This crate is its OWN workspace root. Every cargo command runs from
`crates/sprefa-extract` inside `$PWD` (your worktree), never the repo root and
never another checkout.

Issue: `issues/graph-callers-arm/item.md`. Read it first. It carries the
signatures, the fixture, and the acceptance criteria. Parent epic
`issues/ryi-new-verbs/item.md`.

## COMMIT CONTRACT

One commit per phase, four phases. Every commit carries, one `-m` per line after
the subject:

```
Boop-Status: wip
Boop-Check: <exact command> -> <exact result>
Boop-Trace: <trace file> -> <first perf finding or "none">
Refs-Issue: @graph-callers-arm
```

`Boop-Check` is a command you ran plus its literal output. Never write `rc=0`;
write the counted result, e.g. `190 binaries, 1012 passed, 0 failed`. Last
commit is `Boop-Status: done`. Stuck means `Boop-Status: blocked` plus one
`Boop-Ask`.

## TRACE LAW (every command, no exception)

1. Every `ryi` invocation you run carries
   `HAFLEY_TRACE=$PWD/traces/<phase>-<n>.json RUST_LOG=sprefa_extract=debug`.
   `mkdir -p traces` first. `traces/` is gitignored by you in commit 1
   (`echo traces/ >> .gitignore` inside the crate dir).
2. Every launched command runs under `timeout 10`. A command that hits the
   timeout is a defect; report it as `Boop-Check: ... -> timeout 10 hit`,
   never re-run it with a longer timeout.
3. After every `ryi` run, read the trace before reporting. The chrome timeline
   is a JSON array of events with `name`, `ph`, `ts`, `dur`. Read it as:
   `jq -r '[.[] | select(.ph=="X")] | sort_by(-.dur) | .[0:5] | .[] | "\(.dur) \(.name)"' traces/<f>.json`.
   The first span over 1_000_000 us (1s), or any span name that repeats more
   than the file count of the fixture, is the "first perf error". Write it in
   `Boop-Trace`. If none, write `none`.
4. `resolve_project` runs ONCE per `ryi graph` process. Prove it from the
   trace: count spans named like `resolve*` and put the count in `Boop-Trace`
   of the phase-2 commit.

## Owned files

You may edit ONLY:

- NEW `src/0_graph.rs`
- `src/bin/ryi.rs` (the `mod` block at `:50-69` and the dispatch at
  `:581-613`; re-grep, main moved)
- `src/types.rs` (three new `FlatFact` variants next to `ResolvedEdge`)
- `schema/1_facts.tsp` and whatever `schema/2_gen.mjs` regenerates
- NEW test `tests/149_graph_callers_ts.rs`
- NEW golden under `tests/goldens/` following the neighbouring pattern
- `.gitignore` in the crate dir (one line, `traces/`)

FORBIDDEN: `src/0_rename.rs`, `src/lang/*`, anything under `docs/`, any other
test file, any resolver leg. `--from` and `--uses` arms are later cards; do not
stub them.

## Phases

### Phase 1: the rows

`src/types.rs`: `GraphNode`, `GraphEdge`, `GraphRoot` exactly as the card
lists them. `schema/1_facts.tsp`: the three models. Regenerate, commit the
generated diff. Check: `timeout 10 cargo build --features cli` counted as the
literal last line cargo printed. If the build exceeds 10s cold, run
`timeout 600 cargo build --features cli` ONCE to warm, then the 10s check.

### Phase 2: `GraphCx` and `run_callers`

`src/0_graph.rs` with `GraphCx::load`, `Grade::from_origin`, `run_callers`
per the card's pseudo-code. Wire `#[path]` mod and the `Some("graph")` arm in
`ryi.rs`. Run
`ryi graph --callers deep tests/fixtures/ts5_findings/module_plane` under the
trace law. Paste the summary line (`N edges: ...`) into `Boop-Check`.

### Phase 3: summary line and grade split

`emit_summary_line` prints `N edges: a +, b ~, c -` to stderr after the rows.
`--json` keeps rows only. Verify the grade split on `deep`: the direct caller
via `two_hop_inner` is `+` (module_plane). Record the split you measured.

### Phase 4: test

`tests/149_graph_callers_ts.rs` through the real binary (`CARGO_BIN_EXE_ryi`),
fixture `tests/fixtures/ts5_findings/module_plane`, target `deep`. Assert: row
count, every row has a non-empty `grade`, inline snapshot of the sorted rows.
Pattern: copy from `tests/4_rename_ts.rs`. The test itself sets `HAFLEY_TRACE`
to a tempdir path so the trace law holds under `cargo test`.

Gate: `timeout 600 cargo test --features cli --no-fail-fast` from the crate.
Report the counted result. Baseline at `00a12c6d`: 190 binaries ok, 0 failed.

## Style laws

Match the surrounding file. No em dashes. No `honest`, `load-bearing`,
`substrate`, `provenance`, `regime`. Comments state facts, never narrate.
Tests through the binary, no mocks. `CARGO_TARGET_DIR` is set for you; do not
change it.
