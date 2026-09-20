---
created: 2026-09-18
updated: 2026-09-18
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

- [ ] Design doc lands with type signatures, join pseudo-code, instance lifetimes, storage layout, read/write sequence, uniqueness conditions
- [ ] The six-session chain above reconstructs as one trace
- [ ] Trace survives wrapper death, machine restart, and a claude started outside `boop tui`
- [ ] Join carries an explicit budget with a named diagnostic

## Implementation Notes

Brief written at `TASKS/boop-observation-trace.BRIEF.md`. Design first, no code.
