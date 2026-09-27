---
created: 2026-09-05
updated: 2026-09-27
type: feature
status: done
priority: normal
labels: [domain-boop]
provenance: codex
source_ref: 01a067ea-5289-7092-9d52-3588c1af9555:boop-context-views
closed: 2026-09-27
closed_by: codex
---

# Boop SQL views for favorite context and project conversations

## Description

Expose compact SQL views for favorite context and project-scoped conversations. No new CLI query features. User messages and the preceding assistant explanation determine how saved text should be interpreted.

## Proposed views

- `v_message(session, turn, n, role, body, ts, cwd, source_class, prev_user_n, prev_assistant_n)`: decoded conversational messages, filtered before numbering, retaining source turn IDs and provenance.
- `v_favorite(id, note, body, source, session, turn, n, source_class)`: resolve current and supported historical source spellings; unresolved anchors remain NULL.
- `v_session_project(session, project, evidence)`: membership from cwd or recorded file touches, with path-boundary matching and one row per session/project.

```sql
SELECT m.n-f.n AS offset,m.turn,m.role,m.source_class,m.body
FROM v_favorite f JOIN v_message m USING(session)
WHERE f.id=48 AND m.n BETWEEN f.n-3 AND f.n+3
ORDER BY m.n;
```

## Acceptance Criteria

- [x] Plain SQL retrieves +/-N conversational messages around favorites and favorites from sessions touching a project.
- [x] Filter tools and known harness envelopes before numbering. Preserve source turn IDs and classification; unknown provenance remains explicit.
- [x] Expose repeated assistant entries and specify navigation policy before collapsing rows.
- [x] Resolve supported favorite source formats deterministically; ranges, missing and ambiguous references never fabricate a single anchor.
- [x] Project matching covers cwd-only and touch-only evidence, handles path-boundary prefixes, and avoids duplicated favorite results.
- [x] Tests cover user corrections, interleaved tools, repeated assistants, session boundaries, source classification and unresolved favorites.
- [x] Record EXPLAIN QUERY PLAN and timings for session-scoped windows; avoid accidental whole-store window scans.
- [x] Install views through the schema lifecycle, surviving rebuilds; document short SQL examples without adding query CLI verbs.

## Tests Run

- [x] `CARGO_TARGET_DIR=/Users/chrishafley/.cache/boop/cargo-target cargo nextest run -p boop-store -j 2 -E 'test(/context_views_keep_source_ordinals_and_project_evidence/)'` (1 passed; 0.033s test duration).
- [x] Fixture EXPLAIN QUERY PLAN asserts indexed `SEARCH turn USING PRIMARY KEY` and rejects a full `SCAN turn` for a session-scoped context query.
- [x] The same fixture exercises fresh install, schema v35 to v36 migration, and store rebuild recreation.

## Implementation Notes

Schema v36 installs `v_conversational_turn` as a helper plus `v_message`, `v_favorite`, and `v_session_project`. Favorite sources resolve for `harness:session:assistant:turn` and Instant's `turn:session:turn`; opaque and range sources keep null turn/ordinal anchors. Project candidates are observed session working directories; recorded touches match on `/` boundaries, and evidence is consolidated to one row per session/project. See `crates/boop/docs/9_context-views.md` for SQL examples and the ordinal policy.
