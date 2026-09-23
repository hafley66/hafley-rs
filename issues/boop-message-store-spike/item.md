---
created: 2026-09-23
updated: 2026-09-23
type: improvement
status: open
priority: normal
labels: [domain-boop, intent-design, spike]
size: M
related: ['@lane-close-spam']
---

# Spike: boop messages as lazy frontmatter files behind a MessageStore trait

## Description

Spike + refinement. Boop is imperative today: a caller runs a verb (`boop beep`, `lane create`,
`boop wait`) and the CLI does the side effect immediately (tmux spawn, pane paste, mailbox row,
supervisor). Underneath, boop already writes rows/files that agents read. Make that the model:

- An agent (or a human) WRITES a message: a markdown file with frontmatter.
- The CLI is lazy: it waits on that file/row, validates it (schema, routing, preconditions), and
  runs the side effect the frontmatter names.
- Work is drawn from a declared source, and ordering between cards is data, not a coordinator
  holding the sequence in its head.

## Shape to refine

`trait MessageStore` (name open) with a default filesystem implementation:
- `put(msg) / watch(filter) -> stream / claim(id) / complete(id, rc, detail)`.
- Default impl: markdown + frontmatter files in a directory (the boop mail dir or `issues/`).
- Alternative impl: `issuectl` (already speaks frontmatter issues: `blocked_by:`, `related:`,
  `ready` Definition-of-Done, `note --agent-run`).
- Alternative impl: SQLite behind `~/projects/tiny-serve` (smol + httparse + soketto +
  lsp-server + rusqlite; `src/4_store.rs`, commit 59d894b) for push delivery over one local
  socket instead of polling files.

Frontmatter fields to decide (draft):
| field | meaning |
|---|---|
| `to` / `from` | route (lane, coordinator, human) |
| `kind` | message, card, result, stop |
| `source` | where work is drawn from (issues dir, issuectl query, mailbox) |
| `after` / `blocked_by` | card(s) that must reach `status: done` first |
| `auto_run` | start automatically when `after` resolves |
| `preset` | model preset for the lane that takes it |
| `expect` | completion assertions (paths, commit subjects, commits_at_least) |
| `status` | open / claimed / running / done / failed |

## Questions the spike answers

1. Which boop verbs become "write a file, CLI validates + executes" and which stay imperative?
2. File watch mechanism: FSEvents/inotify via `notify` vs SQLite `data_version` polling vs
   tiny-serve push. Cost of each at idle (see instant issues/kernel-wired-growth: idle polling
   with side effects leaked kernel memory; reads must stay side-effect free).
3. Does `issuectl` cover the card lifecycle (blocked_by, ready, archive) well enough to be the
   default store, or is the boop mail dir the default and issuectl an adapter?
4. Exactly-once side effects: claim/lease semantics so two watchers never both spawn a lane
   (relates to issues/lane-close-spam: one result row per spawn).
5. Minimum tiny-serve surface for messages: endpoints, schema, how agents without HTTP (files
   only) still participate.

## Build vs buy

Before any bespoke queue/scheduler/watcher: a candidate table of existing crates/tools
(file-backed queues, SQLite job queues, frontmatter parsers, `notify`, issuectl itself) with
fit/misfit per requirement.

## Deliverables

- `docs/` or `plans/` design note: trait signature, instance lifetimes, storage + sequence of
  reads/writes and uniqueness conditions (claim), frontmatter schema, verb migration table.
- A throwaway prototype proving: agent writes card -> CLI validates -> side effect runs once ->
  dependent card with `auto_run` starts after it.
- No production boop changes in the spike.

## Related

- issues/lane-close-spam
- instant issues/kernel-wired-growth
