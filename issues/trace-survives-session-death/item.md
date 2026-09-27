---
created: 2026-09-18
updated: 2026-09-27
type: bug
status: open
priority: high
---

# Trace id does not survive clear, exit, compact or resume

## Description

## Description

A boop trace is meant to outlive one harness session id. `crates/boop-proc/src/supervise.rs:2661-2707` states the rule: the conversation id moves on `/clear`, on compaction and on resume; the trace does not. It breaks whenever the process that writes it dies.

## Measurement (2026-09-19, one claude pane in ~/projects/sprefa)

| session | turns | trace | what happened next |
|---|---|---|---|
| `c523b4e9` | 633 | `trace-c523b4e9` | clear, trace carried |
| `59ce15e8` | 478 | `trace-c523b4e9` | clear, trace carried |
| `971d9aad` | 2 | `trace-c523b4e9` | BREAK, new trace minted |
| `c70b92c3` | 432 | `trace-c70b92c3` | clear, trace carried |
| `c848c7e9` | 435 | `trace-c70b92c3` | BREAK on machine restart |
| `e840687a` | 49 | `trace-e840687a` | current |

Three more sprefa claude sessions carry no `agent_trace_span` row at all: `13b4dc23` (483 turns), `4abe8769` (355), `749f31dd` (118).

Store-wide, 3831 of 6200 `agent_session` rows carry a trace.

| attach rule | rows |
|---|---|
| `backfill-spawned-edge` | 1556 |
| `supervisor-conversation` | 1106 |
| `lane-create` | 1033 |
| `native-tui-session` | 126 |
| `lane-run` | 10 |

## Root cause

Only two writers call `attach_trace` (`crates/boop-store/src/ident.rs:2121-2136`): the lane supervisor and the `boop tui` wrapper (`crates/boop/src/cli/control.rs:189-219`). Both must be alive to write. Transcript ingest never calls it, so `SyncDecision::for_session` (`crates/boop/src/cli/db.rs:184-213`) turns a never-seen uuid into a new session row with nothing linking it to its predecessor.

The route name is keyed on the tmux pane (`control.rs:303-311`) and the conversation binding on the pid (`control.rs:120-131`). Both are temporary.

## The link is not in the transcript

Verified by reading `~/.claude/projects/**/*.jsonl`:

- `leafUuid` rides on `type:"last-prompt"` records. It is a within-session bookmark, not a lineage link. Do not build on it.
- `{"type":"continued-in","sessionId":"<old>","continuedInSessionId":"<new>"}` is the real link and is correct when present. It appears **5 times across every project on disk**. None of the breaks above have one.

## User decision (Chris, 2026-09-19)

Record as much relational data as possible at each tick, event and update: pid, parent pid, pane id, tui session id, cwd, harness, wall clock. The trace id is whatever matches an earlier observation according to a join. The trace becomes derived, not asserted by a process that can die.

## Open design questions

- Which identifier overlaps are strong enough to merge traces. Pids wrap and pane ids restart at `%0` after a tmux server dies.
- Per-tick write cost at `BOOP_NATIVE_PROJECT_EVERY_MS` (default 1s) times every live route. Row rate per day and bytes, against the `sqlite-costs` skill.
- `attach_trace` is first-attach-wins today, so there is no path to merge two traces once the join learns they should have been one.
- Backfill over the 6200 existing sessions. The six-session chain above is the acceptance example.
- The join's loop budget and its named diagnostic, per the standing bounded-loop law.

## Acceptance Criteria

- [x] Design doc has type signatures, join pseudo-code, instance lifetimes, storage layout, read/write sequence, uniqueness conditions (`TASKS/boop-observation-trace.BRIEF.md`).
- [x] A six-session fixture chain derives one component while an unrelated session with the same cwd stays separate.
- [x] An explicit Claude `continued-in` transcript relation is stored idempotently and its materialized trace survives closing/reopening the store and `Store::rebuild`.
- [x] Join input has named observation, relation, and session budgets; limit and ambiguous-continuation tests pin diagnostics.
- [x] Schema v37 migration seeds observations from existing `agent_trace_span` rows.
- [ ] The six historical sessions in the description reconstruct as one trace from their available durable observations.
- [ ] Trace survives wrapper death, machine restart, and a Claude session started outside `boop tui` when there is no explicit `continued-in` record. Process and pane joins need host/tmux-server incarnation evidence before they can merge.
- [ ] Measure observation write rate, daily row count, and SQLite bytes on the selected real database.

## Reproduction receipt

Baseline repro before the 2026-09-27 implementation: `rg -n 'continued-in|attach_trace|SyncDecision::for_session' crates/boop/src/cli/db.rs crates/boop-harness/src crates/boop-store/src/ident.rs` finds `SyncDecision::for_session` queues unknown session IDs and trace attachment only in the supervisor/TUI paths; no `continued-in` relation parser or transcript-sync trace writer exists. A restarted writer therefore leaves the newly synced session without a predecessor relation. Existing measurements above are historical corroboration; no external database was opened.

## Implementation Notes

The user directed implementation of the written design. The type, join, storage, lifetime, and gate plan is in `TASKS/boop-observation-trace.BRIEF.md`. The compatibility span projection now handles explicit transcript relations and legacy spans. Host/server-incarnation matching and real-database historical reconstruction remain open.


## Implementation receipt (2026-09-27)

Red: before the fix, ingesting a Claude `type: continued-in` record stored zero relations and left `session-after` without a trace. Green: `continued_in_transcript_relation_is_projected_by_claude_adapter` verifies the Claude adapter decodes the record and calls the shared per-record projector callback; the producer-keyed `ContinuedIn` relation derives/materializes connected membership into the compatibility spans. `continued_in_relation_preserves_trace_after_writer_exit` verifies retry idempotency, close/reopen durability, and `Store::rebuild`. The Claude-specific record check lives in `harness/claude.rs`; `behavioral_harness_dispatch_stays_in_adapters` passes. The six-session pure join fixture, shared-cwd isolation, ambiguous-edge retention without cross-trace merging, all three named join budgets, and the v37 legacy-span migration pass. `cargo nextest run -p boop-store --locked -j 2 --no-fail-fast` passes 241/241; `cargo nextest run --workspace --locked -j 2 -E 'not (test(/e2e|live|tmux|tui_sigint|omp_live/))' --status-level leak --final-status-level fail` passes 1386/1386 with 1 known leak and 203 skipped.

The historical-chain reconstruction and real-database rate/size measurement remain unchecked. Read-only measurement command, with output kept in the lane scratch directory:

```sh
for sample in 1 2; do
  date +%s
  sqlite3 -readonly /Users/chrishafley/.agent/boop.db \
    "SELECT source.value, COUNT(*) FROM agent_session_observation observation JOIN dict_observation_source source ON source.id = observation.source_id GROUP BY source.value ORDER BY source.value; SELECT COUNT(*) AS relations FROM agent_session_relation; PRAGMA page_count; PRAGMA page_size;"
  sleep 60
done > /Users/chrishafley/.cache/lanes/the-gang-graph/trace-db-measurement.txt
```
