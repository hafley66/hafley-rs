# Plan: `boop db search` speed

## Measured cause

`search_turns` (crates/boop-store/src/query.rs) puts `agent_turn` first in the join. `agent_turn` is
`WITHOUT ROWID` keyed `(session_id, turn)`; the only secondary index is
`idx_turn_session_ts(session_id, ts)`. `t.ts >= ?` alone cannot use that index (leading column is
`session_id`), so the plan is `SCAN t`: every row, every `said` byte (1,302,980 rows, 1.50 GB of text),
then `instr(lower(said))` on each.

Real store (read-only, `fails_at`, 3 days): 37,355 turns and 116 of 7,013 sessions in the window.

## Candidates

| approach | query_cost | disk | write_path_impact | schema_change |
|---|---|---|---|---|
| Drive the join from `agent_session`: keep sessions whose `max(ts)` (covering-index lookup on `idx_turn_session_ts`) is in window, then range-seek `idx_turn_session_ts (session_id=?, ts>=?)` | 0.36 s warm (old 4.8 s warm, 16.3 s cold) | 0 | none | no |
| New index `agent_turn(ts)` | range seek on 37k rows, similar to row 1 | ~30-40 MB estimated | one more btree insert per turn | yes (stop) |
| FTS5 external-content table, unicode61 tokenizer | token match only; `fails_at` substring semantics change | ~0.5-1x text (0.75-1.5 GB) estimated | triggers or ingest writes per turn | yes (stop) |
| FTS5 trigram tokenizer | substring match, needle >= 3 chars | ~3x text (~4.5 GB) estimated | triggers or ingest writes per turn | yes (stop) |
| Push `--days`/`--harness` into SQL | already in SQL (`t.ts >= ?2`, `dict_harness.value = ?3`) | 0 | none | no |

Chosen: row 1. Same SQL semantics (`ts >= since`, `instr(lower)`), same `ORDER BY t.ts DESC LIMIT`,
no schema or write-path change. FTS5/ts-index rows are unmeasured estimates and not pursued; they are
schema changes and the target (< 1 s) is met without them.
