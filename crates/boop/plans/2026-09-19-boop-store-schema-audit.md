# boop store schema audit

Measured 2026-09-19 against `~/.agent/boop.db` at schema version 33, 69 tables,
2.1 GB. Every row below was produced by a query against that store, not by
reading migration code.

## Contents

- [Why](#why)
- [Results](#results)
- [D1 one column name, two incompatible meanings](#d1-one-column-name-two-incompatible-meanings)
- [D7 dict_record is 99% unreachable](#d7-dict_record-is-99-unreachable)
- [D10 dict_request is not a dict](#d10-dict_request-is-not-a-dict)
- [D11 three spellings of one foreign key](#d11-three-spellings-of-one-foreign-key)
- [D9 forty unindexed foreign keys](#d9-forty-unindexed-foreign-keys)
- [D2 mixed nullability](#d2-mixed-nullability)
- [D12 CRUD coverage](#d12-crud-coverage)
- [What cleared](#what-cleared)
- [Where the checks live](#where-the-checks-live)
- [Open](#open)

## Why

`crates/boop-types/` reverse-describes the store as OpenAPI 3.1. Writing the
description surfaced contradictions the schema had been carrying silently: the
generator's own assertion that an `x-dict` column must be an integer fired on
two columns, and chasing those two produced the rest of this list.

## Results

| # | check | result |
| --- | --- | --- |
| D1 | one column name, conflicting declared types | **FAIL** `agent_route.session_id` is TEXT, the other 16 are INTEGER |
| D2 | one column name, mixed NOT NULL | **FAIL** 12 names |
| D3 | table with no primary key | pass, 0 of 69 |
| D4 | `dict_*` missing UNIQUE on value | pass, 0 |
| D5 | mixed timestamp suffix inside one table | pass, 0 |
| D6 | dangling FK, an id with no dict row | pass, 0 across 55 FK columns |
| D7 | dict rows nothing points at | **FAIL** `dict_record` 538,548 of 539,358 |
| D8 | epoch seconds vs millis vs the column name | pass, 0 |
| D9 | unindexed foreign keys | **FAIL** 40 |
| D10 | `dict_*` table shape | **FAIL** `dict_request` |
| D11 | FK spelling convention | **FAIL** 3 variants |
| D12 | tables no statement touches | **FAIL** 8 of 69 |

## D1 one column name, two incompatible meanings

`session_id` is the store's spine: a column on 17 of 69 tables. Sixteen hold the
interned integer. One holds the raw string.

```sql
SELECT route, session_id, typeof(session_id) FROM agent_route LIMIT 2;
-- claude-1   301b041f-1016-468d-a2a4-07bfa025ed44   text
-- codex-1151 ses_f63192399ffe5nOZAIwMhFfNea         text

SELECT session_id, typeof(session_id) FROM agent_turn LIMIT 1;
-- 1  integer
```

The failure mode is a wrong answer with no error:

| join | rows |
| --- | --- |
| `agent_route r JOIN dict_session d ON d.id = r.session_id` | **0** |
| `agent_route r JOIN dict_session d ON d.value = r.session_id` | **163** |

166 routes carry a `session_id`. The 3 that miss even the value join are
sessions that were never interned.

Anyone who joins `agent_route` to another `session_id` table by the convention
every other table follows gets an empty result and no diagnostic.

## D7 dict_record is 99% unreachable

`dict_record` interns transcript record UUIDs.

```
rows                        539,358
distinct ids ever referenced      812
unreferenced                538,548   (99%)
on disk                        23.5 MB
```

The only column that reads it is `sync_cursor.record_id_id`, and `sync_cursor`
has 6,241 rows total. Every UUID the projector has ever seen is interned; almost
none is ever pointed at again.

Two other dicts have unreferenced rows, at a scale that looks like ordinary
churn rather than a leak: `dict_session` 297 of 7,901 (3%), `dict_cwd` 1 of
2,211.

## D10 dict_request is not a dict

28 of the 29 `dict_*` tables share one shape. One does not.

```
28x   id INTEGER, value TEXT
 1x   id INTEGER, message_id TEXT, request_id TEXT     dict_request
```

`dict_request` holds 516,925 rows and 27.6 MB. It has no `value` column, so it
cannot participate in the interning contract its name claims. It is a fact table
mapping message to request, wearing a `dict_` prefix.

Together `dict_record` and `dict_request` are 51.1 MB, and neither honours the
convention the other 28 follow.

## D11 three spellings of one foreign key

The `<stem>_id` convention has three live variants:

| dict | rows | referenced as | variant |
| --- | --- | --- | --- |
| `dict_record` | 539,358 | `sync_cursor.record_id_id` | doubled suffix |
| `dict_request` | 516,925 | `agent_usage.request_ref` | `_ref` |
| `dict_pane` | 129 | `agent_live.tmux_pane_id` | prefixed |
| `dict_session` | 7,901 | `session_id` on 17 tables | correct |

This is the same class of problem the cross-system casing rule in `AGENTS.md`
addresses: one logical field, several spellings. The cost here is concrete. A
convention-based reader looking for `<stem>_id` misses the two largest dict
tables in the store entirely, which is exactly what happened while building
`crates/boop-types/gen/er_d2.py`: they landed in an "orphan" bucket holding
1,057,024 rows until the classifier learned all three spellings, after which
orphans fell to 791 rows.

## D9 forty unindexed foreign keys

Largest first:

| column | rows |
| --- | --- |
| `agent_turn.role_id` | 1,022,170 |
| `agent_turn.cwd_id` | 1,022,170 |
| `agent_usage.service_tier_id` | 516,921 |
| `agent_delivery_transition.harness_id` | 308,998 |
| `agent_cmd.program_id` | 222,071 |
| `agent_touch.raw_verb_id` | 125,889 |
| `agent_trace_event.{session,kind,from_lane,to_lane,delivery_state}_id` | 10,000 each |

Interning exists to make these joins integer compares. Without an index on the
referencing side, a lookup by `role_id` is a full scan of a million rows, which
gives back the cost the interning was meant to remove.

## D2 mixed nullability

12 column names are NOT NULL in some tables and nullable in others:

```
session_id    NOT NULL in 12, nullable in 5     agent_turn vs agent_session
model_id      NOT NULL in 1,  nullable in 3     agent_usage vs agent_edge
trace_id      NOT NULL in 1,  nullable in 3     agent_trace_span vs agent_trace
harness_id    NOT NULL in 2,  nullable in 3     agent_session vs agent_lane
ts            NOT NULL in 2,  nullable in 4     agent_usage vs agent_turn
```

Plus `comment_id`, `detail`, `first_ts`, `markdown_id`, `mode`, `started_ts`,
`status_id`.

Several are defensible (a dict's own `id` is the PK; a parent row may legitimately
lack a trace). The list is here so the defensible ones can be marked and the rest
decided, rather than left as an accident.

## D12 CRUD coverage

Across the 327 statements in `crates/boop-types/queries.yaml`, per table:

| operations present | tables |
| --- | --- |
| `CR` only, no update and no delete | 25 |
| `R` only | 14 |
| `CDR` | 8 |
| `CRU` | 5 |
| full `CDRU` | 3 |

Three tables of 69 support the whole cycle. 25 are append-and-read with no path
to correct a row.

Eight tables have no statement touching them at all: `agent_reminder`,
`sync_root_stamp`, `dict_agenttype` (0 rows), `dict_attach`, `dict_service_tier`,
`dict_trace_classification`, `dict_trace_delivery`, `dict_trace_kind`.

CAVEAT: 34 statements are built with `format!` and have no fixed text to prepare,
so they are absent from `queries.yaml`. Any claim that a table is never written
is bounded by that gap. The 14 read-only `dict_*` tables in particular are almost
certainly written through the generic intern helper.

## What cleared

Worth recording, because each was a plausible failure that did not happen.

- **D6, referential integrity.** 0 dangling references across 55 FK columns.
  Every interned id in every fact table resolves to a dict row.
- **D8, epoch units.** No column named `_ts` or `_ms` holds seconds. No mixing.
- **D4, dict uniqueness.** Every `(id, value)` dict carries the UNIQUE.
- **D3, primary keys.** All 69 tables have one.
- **Schemaless storage.** Zero. The two candidates are fixed shapes in a generic
  costume: `agent_session_attr` has had exactly 2 keys ever (`effort` 83 rows,
  `reset_ts` 1), and `agent_trace_event.detail` carries the same 6 fields on
  every row.

## Where the checks live

The audit ran as ad-hoc queries. Nothing re-runs it.

| artifact | status |
| --- | --- |
| `crates/boop-types/gen/relational_yaml.py` | D1 and D10 now assert: a non-integer column carrying `x-dict` emits `x-boop-defect` |
| D2, D3, D4, D5, D6, D7, D8, D9, D11, D12 | not automated |

The `x-boop-defect` assertion is the one that pays for itself: it found D10
without anyone looking for it.

## Open

| question | why it is open |
| --- | --- |
| `agent_route.session_id` | is TEXT deliberate, so a route can name a session before it is interned? If so it needs a different column name |
| `dict_record` | is the 99% unreferenced mass live data for a consumer outside this store, or a leak? |
| `dict_request` | rename to drop the `dict_` prefix, or give it a `value` column |
| the 34 interpolated statements | until they are covered, every "never written" claim has a hole |
| D2's 12 mixed columns | which are deliberate |
| the 40 missing indexes | measure a real query before adding any; a write-heavy table may not want them |
