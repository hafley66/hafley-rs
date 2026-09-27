---
created: 2026-08-24
updated: 2026-08-25
type: epic
owner: chris
status: done
priority: high
labels: [domain-boop]
closed: 2026-08-25
closed_by: claude-5
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

- [x] every child closed
- [x] `boop --help` top level lists at most 9 verbs
- [x] the three live chains in the plan (flash4 side lane; luna>q38>sonnet; codex>native>codex>native) rerun green from the shim binary

## Agent Runs

### 2026-08-25T16:40:37Z · @claude-5

AC 3 on the installed binary (b890b33): side flash4 lane 0c8da22,1c88f74 rc=0; top-luna-2 > mid-q38-2 > leaf-sonnet-2 (zsonnet) 0e660e5 > bc1e3ea > f8a5e94 all rc=0; codex cx-a5 > native-n1e > cx-b5 > native-n2e ping+pong taken, rc=0. boop --help lists 9 verbs. 14 children closed.
