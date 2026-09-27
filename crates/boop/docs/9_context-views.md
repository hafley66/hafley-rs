# Favorite context and project membership SQL views

Boop schema v36 installs three SQL views for transcript context without adding
query CLI commands.

## Conversation ordinals

`v_message` includes user and assistant rows, retains the original `turn`, and
numbers only rows in that conversation as `n`. Tool rows and user rows marked
`source_class = 'harness'` are excluded before numbering. `source_class` remains
visible, so callers can distinguish `human`, `harness`, and `unknown`
provenance. Assistant rows stay distinct, including repeated bodies.

```sql
SELECT n, turn, role, source_class, prev_user_n, prev_assistant_n, body
FROM v_message
WHERE session = 'SESSION_ID'
ORDER BY n;
```

`prev_user_n` and `prev_assistant_n` are the preceding conversation ordinals;
they are NULL until that role has appeared.

## Favorite anchors

`v_favorite` keeps the original `source` and favorite `body`. It resolves the
current `harness:session:assistant:turn` spelling and Instant's historical
`turn:session:turn` spelling. Opaque sources, ranges, missing turns, and turns
filtered from `v_message` have NULL anchors. It does not choose one turn from a
range.

```sql
SELECT m.n - f.n AS offset, m.turn, m.role, m.source_class, m.body
FROM v_favorite f
JOIN v_message m USING (session)
WHERE f.id = 48 AND m.n BETWEEN f.n - 3 AND f.n + 3
ORDER BY m.n;
```

## Project membership

`v_session_project` treats each observed non-empty working directory as a
project root. A session matches when a recorded cwd or touched path equals the
root or lies below it at a `/` path boundary. Relative touches resolve against
their turn cwd. `evidence` is `cwd`, `touch`, or `cwd+touch`; the view returns
one row per session and project.

```sql
SELECT f.id, f.note, f.body, f.session, f.turn, f.n, p.evidence
FROM v_favorite f
JOIN v_session_project p USING (session)
WHERE p.project = '/workspace/project'
ORDER BY f.id;
```
