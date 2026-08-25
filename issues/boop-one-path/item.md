---
created: 2026-08-24
updated: 2026-08-24
type: epic
owner: chris
status: open
priority: high
labels: [domain-boop]
---

# boop: one path per job

## Description

## Description

Boop has one job: a parent knows what its agents did, when, and why, and can push a message that lands. The 2026-08-24 runs (plans/2026-08-24-boop-parent-visibility.PLAN.md, plans/2026-08-24-boop-opencode-supervision-failures.PLAN.md) showed each failure came from a duplicated path: two mail stores, five identity rungs, three model spellings, two codex launchers, four wait verbs. This epic removes the duplicates so the failure classes cannot recur.

Landed before this epic (main ae1057e): supervisor mails every turn end and HEAD move; delivery ladder with a transition per rung; `boop beep <route> <body>` as the one send; `boop debug <lane>`; codex/opencode native subagent cross-messaging proven 4-deep.

## Children, in dispatch order

| order | issue | removes |
|---|---|---|
| 1 | one sqlite mailbox | reconciler, `no delivery transition` rows |
| 2 | env-only identity | wrong-caller sends |
| 3 | presets are the only model spelling | `@medium` rejections, banned-preset spawns |
| 4 | codex-acp is the only codex launcher | double sandbox fixes |
| 5 | one wait | help drift |
| 6 | comment out concatmap and host | 2 top-level verbs |
| 7 | one pane-register path | 4 verbs |
| 8 | door-only claude delivery | `inbox` verb group |
| 9 | db down to sql, chat, status, sync | 3 verbs |
| 10 | hide lane run | 1 verb |

## Acceptance Criteria

- [ ] every child closed
- [ ] `boop --help` top level lists at most 9 verbs
- [ ] the three live chains in the plan (flash4 side lane; luna>q38>sonnet; codex>native>codex>native) rerun green from the shim binary
