---
created: 2026-09-05
updated: 2026-09-07
type: feature
status: untriaged
priority: normal
labels: [domain-boop]
provenance: codex
source_ref: 01a067ea-5289-7092-9d52-3588c1af9555:boop-context-views
---

# Boop SQL views for favorite context and project conversations

## Description

## Request

Expose compact SQL views for favorite context and project-scoped conversations. No new CLI query features. User messages and the preceding assistant explanation determine how saved text should be interpreted.

## Proposed views

- v_message(session, turn, n, role, body, ts, cwd, prev_user_n, prev_assistant_n): decoded messages and per-session message index, excluding tools.
- v_favorite(id, note, body, source, session, turn, n): resolve historical source spellings; unresolved anchors remain NULL.
- v_session_project(session, project, evidence): membership from cwd or recorded file touches, with path-boundary matching.

```sql
SELECT m.n-f.n AS offset,m.role,m.body
FROM v_favorite f JOIN v_message m USING(session)
WHERE f.id=48 AND m.n BETWEEN f.n-3 AND f.n+3
ORDER BY m.n;
```

## Evidence

Favorite 48 points to session 01a04dcb-96a9-7711-80e0-68dbddfa7ac7 turn 124. User turn 122 explicitly requests shared macrotime/comptime/runtime fixpoint, clock, IDB and EDB mechanics. Reading the favorite alone led to misattributing this intent to assistant elaboration. Assistant pairs 120/121, 123/124 and 126/127 are byte-identical.

Related: @boop-human-turn-search. Raw role=user does not establish human authorship; consume its source classification and expose uncertainty.

## Acceptance Criteria

- [ ] Plain SQL retrieves +/-N conversational messages around favorites and favorites from sessions touching a project.
- [ ] Filter tools and known harness envelopes before numbering. Preserve source turn IDs and classification; unknown provenance remains explicit.
- [ ] Expose repeated assistant entries and specify navigation policy before collapsing rows.
- [ ] Resolve supported favorite source formats deterministically; ranges, missing and ambiguous references never fabricate a single anchor.
- [ ] Project matching handles cwd and recorded touches without prefix collisions or duplicated favorite results.
- [ ] Tests cover user corrections, interleaved tools, repeated assistants, session boundaries, source classification and unresolved favorites.
- [ ] Record EXPLAIN QUERY PLAN and timings for session-scoped windows; avoid accidental whole-store window scans.
- [ ] Install views through the schema lifecycle, surviving rebuilds; document short SQL examples without adding query CLI verbs.

## Tests Run

Read-only live-store queries verified schema, favorite joins, duplicate pairs and row_number context. No implementation tests.

## Implementation Notes

Durable favorite-to-message references can replace future source-string parsing while retaining compatibility for historical records. No compiler/kernel work is in scope.

## Comments

### 2026-09-07T19:38:55Z · @codex

User requested recording the Boop and Soopy findings in their respective hafley-rs domains. This remains the Boop finding: compact SQL favorite windows, human-message provenance, and project-context joins. Preserve the existing no-new-query-CLI acceptance criterion. No new Boop execution defect was established by the Soopy peer evaluation. Soopy mutation CLI, dry-run contract, transaction guarantees and producer evidence are separately tracked as @soopy-mutation-cli, @soopy-dryrun-contract, @soopy-transaction-contract and @soopy-producer-receipts. User owns the agent workflow; this request does not authorize introducing a new agent orchestration model.
